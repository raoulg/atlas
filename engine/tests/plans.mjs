// The finest plan Atlas builds of a route, as facts in the language of a `.plan` file, and what its pictures do with
// it: `node plans.mjs PLAN_JS DATA_JSON ROUTE...` prints one JSON object per route. `engine/tests/plans.rs` hands it the
// page's data of a program and compares the facts with the plan written down beside the program.
//
// - `step PATH`, `choice PATH`: an occurrence (a call, by the names on its path from the route) and a choice; a call
//   that may run one of several functions says how many (`step PATH one of N`);
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
// `ATLAS_FINE_BUDGET=n` cuts the plan as a large program's is cut (`plan.js`, `FINE_BUDGET`)
const window = process.env.ATLAS_FINE_BUDGET ? {ATLAS_FINE_BUDGET: Number(process.env.ATLAS_FINE_BUDGET)} : {};
const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + `
return {V, plan, imageOf, coarsen, checkOpenings, overviewOpen, inUnit, partOf, MAKEFILES, treeInfo, stepLabel, oneOf, typeLabel, sideOfRoles, familyOf, kindOf, byId, ordersOn, stepOf, opens, frameOf, valueBox, foldDirs, filePath, valuesOf, closedFamily, typesIn, lightOf, cutBy, familyFrames, here, kindsOf, isData};`)(D, window);
const {V} = M;

const SIDES = {in: "left", out: "right", mid: "between", both: "both"};
// every subset of the steps that open, up to this many steps; beyond it, each step alone and each pair
const ALL_SUBSETS = 6;
// `ATLAS_SAMPLE_OPENINGS=k` (with `ATLAS_SAMPLE_SEED`) asks k sets of open steps of each route, drawn from the seed and the
// route, instead of the sets above: `engine/tests/sample.rs` samples a large program's pictures this way, and
// `ATLAS_SAMPLE_DEADLINE` (milliseconds since the epoch) stops before a route that would start after it
const SAMPLED = Number(process.env.ATLAS_SAMPLE_OPENINGS || 0);
const SEED = Number(process.env.ATLAS_SAMPLE_SEED || 0) >>> 0;
const DEADLINE = Number(process.env.ATLAS_SAMPLE_DEADLINE || Infinity);
// mulberry32: a small generator whose stream a seed decides
function random(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const hash = text => [...text].reduce((h, c) => Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0, 2166136261);

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
  // a value its body binds in a way the plan does not follow is of that body: `main/download/~to`
  const occ = id => {
    if (id.endsWith("|")) return occ(id.slice(0, -1)) + "/?";
    const bound = id.match(/^(n:.*)\/~([^/]+)$/);
    if (bound && !name.has(id)) return `${occ(bound[1])}/~${bound[2]}`;
    return name.get(id.startsWith("o:") ? "n:" + id.slice(2) : id) ?? id;
  };
  // a file by its term, the place's identity; the page's label says the term's patterns in words
  const placeName = k => (k.startsWith("w:File:") ? `File(${k.slice(7).replace(/~[a-z]+$/, "")})` : F.places.get(k)?.label ?? k.replace(/^w:/, "")).replace(/\s+/g, " ");
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
  // a step that is a call into a family says how many functions it may run, as its label does
  for (const n of nodes.values()) {
    if (n.parent === null || n.parent === undefined) continue;
    const one = n.synthetic ? 0 : M.oneOf(n.key);
    facts.push(`${n.synthetic ? "choice" : "step"} ${name.get(n.id)}${M.treeInfo.get(n.key)?.recursive ? " recursive" : ""}${one ? ` one of ${one}` : ""}`);
  }
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
  const faults = [...alternatives(F, occ), ...pictures(F, open, occ, homes, valueName), ...familyOpenings(), ...pickings(), ...folderOpenings([...C.fine.ways.keys()].map(M.filePath).filter(Boolean))];
  if (window.ATLAS_FINE_BUDGET) faults.push(...cutOpenings(F, occ));
  return {facts: [...new Set(facts)].sort(), faults};
}

// A choice in a body is a coproduct of its arms (mathematics.md, "Coproducts"): its frame holds alternatives, and one run takes
// one of them (#357). Of the boxes a choice's frame holds, two in different branches stand in different arms of a choice, as the
// calls say (`Site::arms`), so no run runs both; and every box it holds stands in the arms the frame stands in, so a choice
// inside an arm lies in that arm's frame, and a run through both arms is one run. Asked of the finest plan of every route
function alternatives(F, occ) {
  const out = [], apart = (x, y) => [...x].some(([c, a]) => y.has(c) && y.get(c) !== a);
  const boxes = [...F.frames.values(), ...[...F.pills.values()].filter(P => !P.own)];
  for (const C of F.frames.values()) if (C.synthetic) {
    const kids = boxes.filter(X => X.parent === C.id), raw = C.raw || new Map();
    for (const X of kids) if ([...raw].some(([c, a]) => X.raw?.get(c) !== a)) out.push(`${occ(X.id)} stands in ${occ(C.id)} outside the arms it stands in`);
    kids.forEach((X, i) => kids.slice(i + 1).forEach(Y => {
      if (X.branch !== Y.branch && !apart(X.raw || new Map(), Y.raw || new Map())) out.push(`${occ(X.id)} and ${occ(Y.id)} are two branches of ${occ(C.id)} that run in one run`);
    }));
  }
  return out;
}

// The walk's rule, held where the picture is drawn from: a closed family opens, for each value it stood for, into a frame with
// the id its closed box had, as the page draws the frame of an open family (`valueBox`, #182)
function familyOpenings() {
  const out = [];
  V.open.steps = new Set();
  V.open.families = new Set();
  const closed = new Map();
  for (const p of M.plan().places.values()) if (p.type && p.type !== V.route && /^t:/.test(p.id) && M.opens(p.type)) {
    if (!closed.has(p.type)) closed.set(p.type, []);
    closed.get(p.type).push(p.id);
  }
  for (const [t, boxes] of closed) {
    V.open.families = new Set([t]);
    const frames = new Set([...M.plan().places.values()].filter(p => p.type && M.frameOf(p.type)).map(p => M.valueBox(M.frameOf(p.type), p)));
    for (const b of boxes) if (!frames.has(b)) out.push(`the family ${M.typeLabel(t)}: its box ${b} opens into no frame`);
  }
  V.open.families = new Set();
  M.plan();
  return out;
}

// A picked type is lit where its values flow (#296), and the pick decides what exists before the families group it
// (mathematics.md, "Saturation"): opening the families a pick cuts in two, until none does, leaves no closed box cut in two.
// Opening a family carries its light (refinement keeps what it does not refine): a closed box lit whole opens into a lit
// frame whose kinds are all lit, a dark one into nothing lit; the parts it holds are judged by their own type. And a box is
// lit whole by the pick of what it is: its type's values are all the values it stands for, a family's kinds among them,
// whether a base or a rank makes them its kinds (#306). Asked of every type of the route's fiber the Type lattice draws, a
// class, a family the engine groups or one of its kinds, as the pick of its values
function pickings() {
  const out = [];
  // the boxes of a picture that hold a type, an open family drawn as the page draws it: its frame, the value, and in it a box
  // for each kind the value can be and each part it holds
  const held = () => {
    const places = new Map(M.plan().places);
    for (const F of M.familyFrames(places).values()) {
      places.set(F.id, {id: F.id, type: F.fam, frame: true, fam: F.fam});
      for (const k of F.kids) places.set(k.id, {...k, inFrame: F.id, fam: F.fam});
    }
    return [...places.values()].filter(p => p.type);
  };
  V.open.steps = new Set();
  V.open.families = new Set();
  const closed = held(), families = [...new Set(closed.filter(M.closedFamily).map(p => p.type))];
  for (const p of closed) if (M.lightOf(M.valuesOf(p.type), p) !== "lit") out.push(`the values of ${M.typeLabel(p.type)}: its box ${p.id} is ${M.lightOf(M.valuesOf(p.type), p) || "dark"}`);
  const T = D.flow?.types || {};
  const picks = [...new Set([...D.nodes.filter(n => n.kind === "class" && !n.library).map(n => n.id), ...Object.keys(T.members || {}), ...Object.keys(T.kind || {})])];
  for (const t of picks.filter(t => M.here(t) || M.kindsOf(t).length)) {
    const S = M.valuesOf(t), name = `the values of ${M.typeLabel(t)}`;
    V.open.families = new Set();
    for (let cut = M.cutBy(S, M.plan().places.values()); cut.length; cut = M.cutBy(S, M.plan().places.values())) for (const f of cut) V.open.families.add(f);
    for (const p of M.plan().places.values()) if (M.cutBy(S, [p]).length) out.push(`${name}: the box ${p.id} is cut in two once the families they cut are open`);
    for (const f of families) {
      V.open.families = new Set();
      const was = new Set(held().filter(p => p.type === f && M.closedFamily(p)).map(p => M.lightOf(S, p)));
      if (was.size !== 1 || was.has("litpart")) continue;
      V.open.families = new Set([f]);
      const light = was.has("lit") ? "lit" : null;
      for (const p of held()) if (p.fam === f && !p.part && M.lightOf(S, p) !== light) out.push(`${name}: the family ${M.typeLabel(f)}, ${light ? "lit whole" : "dark"} closed, opens into ${p.id}, which is ${M.lightOf(S, p) || "dark"}`);
    }
  }
  V.open.families = new Set();
  M.plan();
  return out;
}

// The disk is a tree, and the fold of a picture's files into their directories is a quotient of it (mathematics.md,
// "Refinement keeps what it does not refine"): opening a folded directory as the page opens it (a click adds the box's
// directory to the open ones) refines the fold in one step, so that each file the box held stands as itself or in a
// directory below the box, and every other file stands where it stood. Asked of every fold reached by opening one box
// after another (#227), or where a large program's pictures are sampled, of each box of the closed fold; that closing
// it draws the box again is asked of the page, which closes it (`pictures.py topology`)
function folderOpenings(paths) {
  const out = [], seen = new Set();
  const fold = open => { V.open.dirs = new Set(open); return M.foldDirs(paths).to; };
  const walk = open => {
    const key = [...open].sort().join(" ");
    if (seen.has(key) || seen.size > 256) return;
    seen.add(key);
    const A = fold(open);
    for (const d of new Set(A.values())) {
      const B = fold([...open, d]), at = `opening the folder ${d}${open.length ? ` with ${open.join(", ")} open` : ""}`;
      for (const p of paths) {
        const a = A.get(p) || p, b = B.get(p) || p;
        if (a === d ? !(b === p || (b.startsWith(d) && b !== d)) : b !== a) out.push(`${at}: ${p} stands in ${b}, closed in ${a}`);
      }
      if (!SAMPLED) walk([...open, d]);
    }
  };
  walk([]);
  V.open.dirs = new Set();
  return out;
}

// Opening a step beyond the cut plans it anew, unfolded one level more: the picture with it open is drawn from another plan
// than the one it was cut in, so the oracle, which opens a box of one plan, does not hold it. Here each step of the closed
// picture that stands beyond the cut is opened as the page opens it, and the picture it opens to must refine the one it was
// cut in (mathematics.md, "Refinement keeps what it does not refine"): seen through the quotient map, which sends what lies
// inside the opened step and the values made at moments inside it to the step's box, it takes no line away and draws no line
// the cut step did not (#183)
function cutOpenings(Fc, occ) {
  const route = V.route, foldable = ["cli", "script"].includes(M.byId.get(route)?.entry?.kind), out = [];
  const lines = view => new Set([...view.inc.values()].filter(i => !view.folded.has(i.c)).map(i => (i.way === "D" ? [i.c, i.p] : [i.p, i.c])));
  V.open.steps = new Set();
  const A = M.imageOf(Fc, k => k === route, foldable), was = new Set([...lines(A)].map(([a, b]) => `${a}>${b}`));
  const cut = [...Fc.pills.values()].filter(P => P.cut && A.pillBlock.get(P.id) === P.id).map(P => P.steps[0]);
  for (const key of cut) {
    V.open.steps = new Set([key]);
    M.plan();
    const Fo = window.atlasPlan.fine, B = M.imageOf(Fo, k => k === route || k === key, foldable), box = "n:" + key;
    const below = id => [key + "/", key + "|", key + "~c"].some(p => id.startsWith("n:" + p) || id.startsWith("o:" + p));
    const phi = id => {
      if ([box, "o:" + key].includes(id) || below(id)) return box;
      // a value made at a moment inside the step, or read back inside it, is the step's own
      const at = /^[tv]:/.test(id) && B.places.get(id)?.at;
      if (at && below(at)) return box;
      return A.folded.get(id) || id;
    };
    const now = new Set();
    for (const [a, b] of lines(B)) { const x = phi(a), y = phi(b); if (x !== y) now.add(`${x}>${y}`); }
    const at = `opening cut step ${occ(box)}`;
    for (const l of was) if (!now.has(l)) out.push(`${at}: lost ${l}`);
    for (const l of now) if (!was.has(l)) out.push(`${at}: more ${l}`);
  }
  V.open.steps = new Set();
  M.plan();
  return out;
}

// the faults of the pictures of one plan, for every set of open steps
function pictures(F, openable, occ, homes, valueName) {
  const route = V.route, steps = [...openable].filter(s => s !== route);
  const foldable = ["cli", "script"].includes(M.byId.get(route)?.entry?.kind);
  const draw = random(SEED ^ hash(route));
  const sets = SAMPLED
    ? [[], ...[...Array(SAMPLED).keys()].map(() => { const p = draw(); return steps.filter(() => draw() < p); })]
    : steps.length <= ALL_SUBSETS
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
  // sampled, a large program's route is asked of the sets drawn for it; in full, of none, each step alone and all
  faults.push(...closing(SAMPLED ? sets : [[], ...steps.map(s => [s]), ...(steps.length > 1 ? [steps] : [])], occ));
  V.open.steps = new Set();
  return faults;
}

// A picture as what it draws: its boxes, the frames they stand in, the lines of each step with their counts, and the side
// of each place of the world
function drawn(C) {
  const sorted = m => [...m].map(([k, v]) => [k, v]).sort();
  const pills = [...C.pills.values()].map(P => [P.id, sorted(P.Dn || []), sorted(P.Cn || []), P.folded || 0, P.backTo || ""]).sort();
  const frames = [...C.frames].map(([id, f]) => [id, f.parent || ""]).sort();
  return JSON.stringify({places: [...C.places.keys()].sort(), pills, frames, sides: sides(C)});
}

// where each place of the world stands, decided on the coarsest picture (`coarsen`)
function sides(C) {
  return [...C.roles.keys()].sort().map(k => {
    const w = C.fine.ways.get(k);
    return [k, M.sideOfRoles([C.roles.get(k)], () => (w?.size === 1 ? [...w][0] : "mid"))];
  });
}

// Closing (mathematics.md, "Refinement keeps what it does not refine"): the picture is a function of the set of open steps,
// not of the order they were opened in. Each set (none, each step alone, all of them; sampled, the sets drawn) is drawn as
// the page draws it (`plan`), once in order and once more after every other set was drawn, in the reverse order: both draw
// the same picture, so closing a step draws the picture it was. Side: a place stands where the coarsest picture puts it,
// so it stands there whatever is open.
function closing(sets, occ) {
  const out = [], first = new Map(), name = set => (set.length ? `with ${set.map(s => occ("n:" + s)).join(", ")} open` : "closed");
  for (const set of sets) {
    V.open.steps = new Set(set);
    first.set(set, drawn(M.plan()));
  }
  const closed = JSON.stringify(JSON.parse(first.get(sets[0])).sides);
  for (const set of [...sets].reverse()) {
    V.open.steps = new Set(set);
    const again = drawn(M.plan()), was = first.get(set);
    if (again !== was) out.push(`${name(set)}: drawn again after the other sets, the picture differs`);
    const s = JSON.stringify(JSON.parse(was).sides);
    if (set.length && s !== closed) out.push(`${name(set)}: a place stands on another side than with every step closed: ${s} against ${closed}`);
  }
  return out;
}

// The image of the fine lines under the quotient, as `mathematics.md` defines it: a fine occurrence goes to its outermost
// closed step (or stays itself); a value made below a closed step and taken in it is internal to it, and so is a read of
// a place the step wrote itself; the other lines between two blocks are one line per channel that counts them. A line
// comes from the block itself when its own arrow supplies it (what the step's call is handed, what its own arrow takes
// and gives) and reaches through it otherwise. A step beyond the cut is the image of its body: each of its lines stands
// for the fine lines its body draws there (`mult`), of which some it reaches. The picture must draw exactly these counts,
// and a value it draws stands where it stands with every step open.
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
  const want = new Map(), add = (b, way, c, n, r) => {
    const k = JSON.stringify([b, way, c]), e = want.get(k) || {n: 0, r: 0};
    e.n += n; e.r += r;
    want.set(k, e);
  };
  for (const P of F.pills.values()) {
    if (view.hidden.has(P.id)) continue;
    const S = closedTop(P.id), b = S || P.id;
    if (view.pillBlock.get(P.id) !== b) out.push(`${occ(P.id)} lies in ${occ(view.pillBlock.get(P.id) || "")}, the quotient puts it in ${occ(b)}`);
    const step = S && F.frames.get(S).step, mine = !S || P.id === "o:" + step;
    const back = new Set(S ? M.ordersOn(M.stepOf(step), M.treeInfo.get(step)?.on).reads_back || [] : []);
    const hands = S ? M.treeInfo.get(step)?.site?.hands || [] : [];
    const handed = pl => !!pl && hands.some(h => (h.name && !h.name.startsWith("#") ? h.name === pl.name : !!h.type && h.type === pl.type));
    for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const [k, r] of m) {
      const pl = F.places.get(k), value = /^[tv]:/.test(k);
      if (S && value && below(pl?.at, S) && !view.hidden.has(pl.at)) continue;
      if (S && way === "D" && !value && back.has(k)) continue;
      // a line of a step beyond the cut counts the fine lines its body draws there, and of them the ones it reaches; the
      // block reaches all of them where it reaches through the step, and the ones the step reaches where it is the step
      const far = mine ? r : !(way === "D" && value && handed(pl)), [n, nr] = P.mult?.[way]?.get(k) || [1, r ? 1 : 0];
      add(b, way, view.q(k), n, mine ? nr : far ? n : 0);
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

// The overview is the image of one fine plan `F₀` of generators (mathematics.md, "One fine plan, every picture its image"):
// the functions of the code, each with its own arrow, a type as one place, the places of the world, and the calls between
// functions, a channel of their own. For every set of open packages and parts (each subset while there are few, else each
// alone, each pair and all), the picture is asked of the quotient as the definition takes it: a function goes to its
// outermost closed package, a type is internal to a closed package that holds every function that makes or takes it, and
// each other line counts the fine lines it stands for, none of them reached; a call goes between the blocks of its two ends,
// and is internal to one that holds both. Opening one more box refines the picture (the oracle, `checkOpenings`), and the
// picture is a function of the set of open boxes, not of the order they were opened in. All of it is asked twice: of the
// overview with the test code, and of the overview without it, whose fine plan is the fiber of the first (`testFaults`)
function overviewFaults(tests) {
  V.route = null; V.kind = null; V.part = null; V.focus = new Set(); V.tests = tests;
  for (const k of Object.keys(V.open)) V.open[k] = new Set();
  M.plan();
  const F = window.atlasPlan.fine, out = arrowFaults(window.atlasPlan.fine, tests), sets = openings(F), pre = tests ? "" : "without the test code, ";
  const first = new Map();
  for (const set of sets) {
    openAs(set);
    const at = pre + (set.length ? `the overview with ${set.join(", ")} open` : "the overview closed");
    const view = M.imageOf(F, M.overviewOpen, false);
    for (const f of M.checkOpenings(F, view, false, M.overviewOpen)) out.push(`${at}: ${f}`);
    // the block of a fine occurrence: its outermost closed frame, else itself
    const block = id => {
      let top = null;
      for (let f = F.pills.get(id).parent; f; f = F.frames.get(f)?.parent) if (!M.overviewOpen(F.frames.get(f).step)) top = f;
      return top || id;
    };
    const inside = (home, S) => { for (let f = home; f; f = F.frames.get(f)?.parent) if (f === S) return true; return false; };
    const want = new Map();
    for (const P of F.pills.values()) for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const k of m.keys()) {
      const b = block(P.id), pl = F.places.get(k);
      if (b !== P.id && inside(pl?.home, b)) continue;
      const key = JSON.stringify([b, way, view.q(k)]);
      want.set(key, (want.get(key) || 0) + 1);
    }
    const drawn = new Map();
    for (const B of view.pills.values()) for (const [way, n, r] of [["D", B.Dn, B.Dr], ["C", B.Cn, B.Cr]]) for (const [c, k] of n || []) {
      drawn.set(JSON.stringify([B.id, way, c]), k);
      if (r?.get(c)) out.push(`${at}: ${B.id} reaches ${c}, a line of its own code`);
    }
    for (const [k, n] of want) if (drawn.get(k) !== n) out.push(`${at}: the line ${k} counts ${drawn.get(k) ?? "nothing"}, the fine lines ${n}`);
    for (const k of drawn.keys()) if (!want.has(k)) out.push(`${at}: the line ${k} stands for no fine line`);
    const calls = new Set(F.calls.map(([a, b]) => [block("n:" + a), block("n:" + b)]).filter(([x, y]) => x !== y).map(([x, y]) => `${x}>${y}`));
    const drawnCalls = new Set([...view.pills.values()].flatMap(P => [...(P.calls || [])].map(b => `${P.id}>${b}`)));
    for (const c of calls) if (!drawnCalls.has(c)) out.push(`${at}: no call ${c}`);
    for (const c of drawnCalls) if (!calls.has(c)) out.push(`${at}: the call ${c} stands for no call of the code`);
    first.set(set, drawnOf(M.plan()));
  }
  for (const set of [...sets].reverse()) {
    openAs(set);
    if (drawnOf(M.plan()) !== first.get(set)) out.push(`${pre}the overview with ${set.join(", ")} open: drawn again after the other sets, the picture differs`);
  }
  for (const k of Object.keys(V.open)) V.open[k] = new Set();
  return out;
}
function drawnOf(C) {
  return JSON.stringify([drawn(C), [...C.pills.values()].map(P => [P.id, [...(P.calls || [])].sort()]).sort()]);
}
// A function's own arrow in `F₀` is its morphism (sentence 2): its occurrence takes every type of data the function takes
// and gives every one it makes (gives back, builds, changes), whether or not another function of the code makes or takes
// that type. Asked of the program's own functions in a package the overview holds, with the test code and without it
function arrowFaults(F, tests) {
  const out = [], pre = tests ? "" : "without the test code, ";
  const leaf = new Set([...F.frames.keys()]);
  for (const e of D.edges) {
    const way = e.kind === "takes" ? "D" : ["returns", "instantiates", "mutates"].includes(e.kind) ? "C" : null, n = M.byId.get(e.src);
    if (!way || !n || n.library || !["function", "method"].includes(n.kind) || (!tests && testCode(n)) || !M.isData(e.dst)) continue;
    const P = F.pills.get("n:" + e.src);
    if (P && !leaf.has(P.parent)) continue;
    if (!P?.[way].has("t:" + e.dst)) out.push(`${pre}${M.stepLabel(e.src)} ${way === "D" ? "takes" : "makes"} ${M.typeLabel(e.dst)}, and its occurrence in the overview's fine plan does not`);
  }
  return out;
}
// the sets of open boxes an overview's plan is asked at: each subset while there are few, else each alone, each pair and all
function openings(F) {
  const boxes = [...F.frames.values()].map(f => f.step);
  return boxes.length <= ALL_SUBSETS
    ? [...Array(1 << boxes.length).keys()].map(m => boxes.filter((_, i) => m & (1 << i)))
    : [[], ...boxes.map(b => [b]), ...boxes.flatMap((a, i) => boxes.slice(i + 1).map(b => [a, b])), boxes];
}
function openAs(set) {
  V.open.units = new Set(set.filter(s => s.startsWith("unit:")).map(s => s.slice(5)));
  V.open.parts = new Set(set.filter(s => s.startsWith("part:")).map(s => s.slice(5)));
}

// Test code, as mathematics.md names the predicate ("A filter: the program without its test code"), restated here from the
// graph: a test, code a reader marks as test code, or code in a test module
const testCode = n => !!n && (!!n.test || n.entry?.kind === "test" || !!M.byId.get(n.kind === "module" ? n.id : n.module)?.test);
// The fiber of the overview's fine plan `F₀` where the code is no test code, taken on the plan: the occurrences of functions
// that are no test code, of their lines those whose formula (`whole`: who makes the touch's path whole) holds with the test
// code false, the calls between two of them, the packages and parts that hold a module of the program, and of the places
// those that a line kept touches, a type's home the smallest package that holds every occurrence kept that makes or takes
// a type of its family, as `F₀` defines it
function fiberOf(F) {
  const lineKept = (P, k) => { const w = P.whole?.get(k); return w == null || w.some(c => c.every(g => !testCode(M.byId.get(g)))); };
  const kept = [...F.pills].filter(([, P]) => !testCode(M.byId.get(P.steps[0])));
  const pills = new Map();
  for (const [id, P] of kept) {
    const only = m => new Map([...m].filter(([k]) => lineKept(P, k)));
    const D = only(P.D), C = only(P.C);
    pills.set(id, {...P, D, C, goes: new Map([...P.goes].filter(([k]) => D.has(k) || C.has(k)))});
  }
  const program = [...M.byId.values()].filter(n => n.kind === "module" && !n.library && !testCode(n));
  const holds = step =>
    step.startsWith("part:") ? program.some(m => M.partOf(m) === step.slice(5)) : M.MAKEFILES.has(step.slice(5)) || program.some(m => M.inUnit(m.id, step.slice(5)));
  const frames = new Map([...F.frames].filter(([, f]) => holds(f.step)));
  const chainOf = fid => { const c = []; for (let f = fid; f; f = frames.get(f)?.parent) c.unshift(f); return c; };
  const places = new Map();
  // a type's home holds every occurrence kept that makes or takes a type of its family
  const kin = k => { const f = F.places.get(k)?.type && M.familyOf(F.places.get(k).type); return f ? [...F.places.keys()].filter(j => F.places.get(j)?.type && "home" in F.places.get(j) && M.familyOf(F.places.get(j).type) === f) : [k]; };
  for (const [k, pl] of F.places) {
    const by = [...pills.values()].filter(P => P.D.has(k) || P.C.has(k));
    if (!by.length) continue;
    if (!("home" in pl)) { places.set(k, pl); continue; }
    const family = kin(k), all = [...pills.values()].filter(P => family.some(j => P.D.has(j) || P.C.has(j)));
    let common = null;
    for (const c of all.map(P => chainOf(P.parent))) {
      if (!common) { common = c; continue; }
      let i = 0;
      while (i < common.length && i < c.length && common[i] === c[i]) i++;
      common = common.slice(0, i);
    }
    places.set(k, {...pl, home: common?.length ? common[common.length - 1] : undefined});
  }
  const calls = F.calls.filter(([a, b]) => pills.has("n:" + a) && pills.has("n:" + b));
  // an occurrence stands where it draws a line or a call
  const called = new Set(calls.flat().map(f => "n:" + f));
  for (const [id, P] of pills) if (!P.D.size && !P.C.size && !called.has(id)) pills.delete(id);
  return {...F, places, pills, frames, calls};
}
// a fine plan of the overview as data that does not depend on the order it was built in
function planOf(F) {
  const keys = m => [...m.keys()].sort();
  return JSON.stringify({
    pills: [...F.pills.values()].map(P => [P.id, P.parent || "", keys(P.D), keys(P.C)]).sort(),
    places: [...F.places.values()].map(p => [p.id, p.home || ""]).sort(),
    frames: [...F.frames.values()].map(f => [f.id, f.parent || ""]).sort(),
    calls: F.calls.map(c => c.join(">")).sort(),
    found: [...F.found].filter(k => F.places.has(k)).sort(),
    updated: [...F.updated].filter(k => F.places.has(k)).sort(),
  });
}
// A filter is a fiber (sentence 5): the overview without the test code is the image of the fiber of `F₀` where the code is no
// test code, the fiber taken first and the quotient taken of what is left. Its fine plan is the fiber of the plan with the test
// code, and for every set of open boxes its picture is the image of that fiber. A package that holds test code and ordinary
// code side by side is a class the predicate cuts in two: a box both pictures draw, whose lines differ, is a counterexample to
// taking the quotient first, which keeps or drops a box whole (the fact `cut BOX`)
function testFaults() {
  const out = [], facts = new Set();
  V.route = null; V.kind = null; V.part = null; V.focus = new Set();
  for (const k of Object.keys(V.open)) V.open[k] = new Set();
  V.tests = true; M.plan();
  const fiber = fiberOf(window.atlasPlan.fine), sets = openings(window.atlasPlan.fine);
  V.tests = false; M.plan();
  const kept = window.atlasPlan.fine;
  if (planOf(kept) !== planOf(fiber)) out.push(`without the test code, the fine plan is not the fiber of the one with it: ${planOf(kept)} against ${planOf(fiber)}`);
  const lines = B => JSON.stringify([[...(B.Dn || [])].sort(), [...(B.Cn || [])].sort(), [...(B.calls || [])].sort()]);
  for (const set of sets) {
    openAs(set);
    const at = set.length ? `the overview with ${set.join(", ")} open` : "the overview closed";
    V.tests = true;
    const withTests = M.plan();
    V.tests = false;
    const without = M.plan();
    if (drawnOf(M.coarsen(fiber, M.overviewOpen, () => false)) !== drawnOf(M.coarsen(window.atlasPlan.fine, M.overviewOpen, () => false)))
      out.push(`without the test code, ${at}: the picture is not the image of the fiber`);
    for (const [id, B] of without.pills) if (withTests.pills.has(id) && /^n:(unit|part):/.test(id) && lines(B) !== lines(withTests.pills.get(id))) facts.add(`cut ${id.slice(2)}`);
  }
  for (const k of Object.keys(V.open)) V.open[k] = new Set();
  return {facts: [...facts].sort(), faults: out};
}

// with no route named, every route of the program but its tests; `--overview` asks the overview, with the test code and without
if (routes[0] === "--overview") {
  const fiber = testFaults();
  process.stdout.write(JSON.stringify({"@overview": {facts: fiber.facts, faults: [...overviewFaults(true), ...overviewFaults(false), ...fiber.faults]}}));
  process.exit(0);
}
const named = routes.length ? routes : [...M.byId.values()].filter(n => n.entry && n.entry.kind !== "test").map(n => n.id);
const result = {};
for (const r of named) {
  if (Date.now() > DEADLINE) break;
  result[r] = read(r);
}
process.stdout.write(JSON.stringify(result));
