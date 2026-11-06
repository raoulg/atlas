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
// still be opened refines the picture), the quotient's (each line of the picture is the image of the fine lines
// that cross a block's border, by channel and count) and the merge's (each line of the picture the page draws, its
// siblings merged, has both its ends among its boxes: `ends`).
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
return {V, plan, imageOf, coarsen, checkOpenings, overviewOpen, inUnit, partOf, MAKEFILES, treeInfo, stepLabel, oneOf, typeLabel, sideOfRoles, familyOf, kindOf, byId, ordersOn, stepOf, opens, frameOf, valueBox, foldDirs, filePath, valuesOf, closedFamily, typesIn, lightOf, cutBy, familyFrames, here, kindsOf, isData, worldPlaceOf, placeOfType, focusOpen: typeof focusOpen === "function" ? focusOpen : null, overviewEntries, planBase, VIEWS, viewOf, dirsOf};`)(D, window);
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
  const {nodes, name, occ, valueName, values, placeName} = namesOf(F);
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
  const open = new Set([...F.frames.values()].filter(f => f.step && !f.synthetic).map(f => f.step)), homes = homesOf(F, values);
  for (const [k, h] of homes) facts.push(`home ${values.get(k)} ${h ? occ(h) : "outside"}`);
  // where a place of the world stands, decided on the closed picture
  for (const k of new Set([...F.pills.values()].flatMap(P => [...P.D.keys(), ...P.C.keys()]))) {
    if (k.startsWith("e:")) { facts.push(`side entry left`); continue; }
    if (!k.startsWith("w:")) continue;
    const w = C.fine.ways.get(k), s = M.sideOfRoles([C.roles.get(k)].filter(Boolean), () => (w?.size === 1 ? [...w][0] : "mid"));
    facts.push(`side ${placeName(k)} ${SIDES[s]}`);
  }
  const faults = [...alternatives(F, occ), ...pictures(F, open, occ, homes, valueName), ...familyOpenings(), ...pickings(), ...folderOpenings([...C.fine.ways.keys()].map(M.filePath).filter(Boolean))];
  V.open.steps = new Set();
  faults.push(...orderings(() => [M.plan(), M.coarsen(window.atlasPlan.fine)]));
  if (window.ATLAS_FINE_BUDGET) faults.push(...cutOpenings(F, occ));
  return {facts: [...new Set(facts)].sort(), faults};
}

// The names the facts of a route's plan use: an occurrence by the labels on its path from the route, a value by its type, its
// name and the occurrence that made it, a place of the world by its term. Every value a line of the plan touches is named, in
// the order of the lines, so that two values the names do not tell apart are told apart by their count alike in every use
function namesOf(F) {
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
  for (const P of F.pills.values()) for (const k of [...P.D.keys(), ...P.C.keys()]) valueName(k);
  return {nodes, name, occ, valueName, values, placeName};
}
// where each value of the plan stands with every step open, each type as itself
function homesOf(F, values) {
  const was = V.open.families;
  V.open.families = new Set([...F.places.values()].filter(p => p.type).flatMap(p => [M.familyOf(p.type), M.kindOf(p.type)]));
  const all = M.imageOf(F, () => true, false), homes = new Map();
  for (const [k] of values) if (/^[tv]:/.test(k)) homes.set(k, all.homeOf([all.q(k)], false));
  V.open.families = was;
  return homes;
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
    const C = M.plan();
    for (const f of ends(C, M.coarsen(window.atlasPlan.fine))) faults.push(`${at}: ${f}`);
  }
  // sampled, a large program's route is asked of the sets drawn for it; in full, of none, each step alone and all
  faults.push(...closing(SAMPLED ? sets : [[], ...steps.map(s => [s]), ...(steps.length > 1 ? [steps] : [])], occ));
  V.open.steps = new Set();
  return faults;
}

// The picture the page draws is the coarse plan with its siblings that draw alike merged into one box (`siblings`), a quotient
// of its steps, and a quotient keeps the image of every line (#412). So every line of the picture has both its ends among its
// boxes: a line of data goes between a step and a place the picture draws, a call and a line back between two steps it draws.
// And siblings are one box only where they draw the same lines: on the picture before the merge (`unmerged`), the members of
// a box draw the same lines of data, call the same steps and are called by the same ones
function ends(C, unmerged) {
  const out = [];
  for (const P of C.pills.values()) {
    for (const k of P.D.keys()) if (!C.places.has(k)) out.push(`the line ${k} > ${P.id} starts at no box the picture draws`);
    for (const k of P.C.keys()) if (!C.places.has(k)) out.push(`the line ${P.id} > ${k} ends at no box the picture draws`);
    for (const b of P.calls || []) if (!C.pills.has(b)) out.push(`the call ${P.id} > ${b} ends at no box the picture draws`);
    if (P.backTo && !C.pills.has(P.backTo)) out.push(`the line back ${P.id} > ${P.backTo} ends at no box the picture draws`);
  }
  const callers = new Map();
  for (const P of unmerged.pills.values()) for (const b of P.calls || []) { if (!callers.has(b)) callers.set(b, []); callers.get(b).push(P.id); }
  const lines = P => `${P.fine ?? [...P.D.keys()].sort() + " > " + [...P.C.keys()].sort()}; calls ${[...(P.calls || [])].sort()}; called by ${(callers.get(P.id) || []).sort()}`;
  for (const Q of C.pills.values()) {
    if (Q.steps.length < 2) continue;
    const members = [...unmerged.pills.values()].filter(P => P.parent === Q.parent && P.steps.every(s => Q.steps.includes(s)));
    const drawn = new Set(members.map(lines));
    if (drawn.size > 1) out.push(`the box ${Q.id} joins siblings that draw other lines: ${members.map(P => `${P.id} draws ${lines(P)}`).join(", ")}`);
  }
  return out;
}

// The places on the disk are ordered by their terms (`D.flow.lies_in`, from the top), and the order is a quotient of them whose
// blocks are the folders and patterns the program touches (mathematics.md, "Paths as terms"; #381). So a place the picture draws
// stands in the outermost place of its order that the picture draws and has not opened: closed, that place's box stands for
// it, and no box of its own is drawn; opened, the place stands in its frame again, as refinement keeps what it does not refine.
// Asked with the folders and patterns closed, each opened alone, and all opened; `picture` draws the picture and the one before
// the order is taken
const LIES_IN = Object.entries(D.flow?.lies_in || {});
function orderings(picture) {
  if (!LIES_IN.length) return [];
  const out = [], holders = [...new Set(LIES_IN.flatMap(([, qs]) => qs))].map(q => q.slice(7)), was = new Set(V.open.dirs);
  for (const set of [[], ...holders.map(h => [h]), ...(holders.length > 1 ? [holders] : [])]) {
    V.open.dirs = new Set(set);
    const [C, before] = picture(), at = set.length ? `with ${set.join(", ")} open` : "with every folder closed";
    const drawn = new Set([...before.places.keys()].filter(k => k.startsWith("w:File:")));
    const box = k => (C.meets?.frames.has(k) ? k : C.places.has(k) ? k : null);
    for (const p of drawn) {
      const hs = (D.flow.lies_in[p] || []).filter(q => q !== p && drawn.has(q)), c = hs.find(q => !V.open.dirs.has(q.slice(7)));
      if (c) {
        if (C.places.has(p)) out.push(`${at}: ${p} lies in ${c}, which is closed, and is drawn as a box of its own`);
        if (!C.places.get(c)?.members?.includes(p)) out.push(`${at}: ${p} lies in ${c}, which is closed, and its box does not stand for it`);
      } else if (hs.length) {
        const q = hs[hs.length - 1], b = box(p);
        if (!b || !C.meets?.frames.get(q)?.kids.has(b)) out.push(`${at}: ${p} lies in ${q}, which is open, and stands outside its frame`);
      }
    }
  }
  V.open.dirs = was;
  return out;
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
    const C = M.plan();
    for (const f of ends(C, M.coarsen(window.atlasPlan.fine, M.overviewOpen, () => false))) out.push(`${at}: ${f}`);
    first.set(set, drawnOf(C));
    for (const f of orderings(() => [M.plan(), M.coarsen(window.atlasPlan.fine, M.overviewOpen, () => false)])) out.push(`${at}: ${f}`);
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
  // a write of data whose path a caller makes whole (`Touch::fixed_by`) is the callee's own touch, at the place the caller's
  // path names: it stands on the callee's occurrence, never on the caller's and never nowhere. A touch that only gives the
  // place its shape (a directory made) serves the data written there, and is drawn through it
  for (const n of M.byId.values()) {
    if (n.library || !["function", "method"].includes(n.kind) || (!tests && testCode(n))) continue;
    for (const t of n.touches || []) {
      if (t.kind !== "disk_write" || t.shape || t.test || !t.artifact || !(t.fixed_by || []).length || (!tests && t.fixed_by.every(f => testCode(M.byId.get(f))))) continue;
      const k = M.worldPlaceOf(t, true).id, P = F.pills.get("n:" + n.id);
      if (!P?.C.has(k)) out.push(`${pre}${M.stepLabel(n.id)} writes ${k}, whose path ${t.fixed_by.join(", ")} hands it, and its occurrence in the overview's fine plan does not`);
    }
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

// A focus is the fiber of `F₀` over the packages in it, with the rest of the code around it (mathematics.md, "One fine plan,
// every picture its image", the focus). Restated here from `F₀` and the packages in focus `U`: the functions of the packages in
// focus with every place they make, take or touch, a type with every type of its kind (the box the overview draws is one
// block of `F₀`'s places); every other function with its lines to those places and its calls into the focus, and where it
// draws neither it stands nowhere; the frames of the focus as in `F₀`, those in focus that lie in no package in focus at the
// top, and the rest's: each package that holds no code in focus and lies in none that holds none, in the rest's frame; a
// type's home `F₀`'s where it is a package of the focus, else outside every package; the calls with an end in the focus
const REST_FRAME = "n:unit:~rest";
function starOf(F0, U) {
  const unitAt = fid => (F0.frames.get(fid)?.step.startsWith("unit:") ? F0.frames.get(fid).step.slice(5) : null);
  const inFocus = fid => { for (let f = fid; f; f = F0.frames.get(f).parent) if (U.includes(unitAt(f))) return true; return false; };
  const holds = fid => [...F0.frames.keys()].some(g => inFocus(g) && (() => { for (let f = g; f; f = F0.frames.get(f).parent) if (f === fid) return true; return false; })());
  const kind = k => (F0.places.get(k)?.type ? M.kindOf(F0.places.get(k).type) : k);
  const mine = [...F0.pills.values()].filter(P => inFocus(P.parent));
  const kinds = new Set(mine.flatMap(P => [...P.D.keys(), ...P.C.keys()]).map(kind));
  const star = k => kinds.has(kind(k));
  const calls = F0.calls.filter(c => c.some(f => inFocus(F0.pills.get("n:" + f)?.parent)));
  const called = new Set(calls.flat().map(f => "n:" + f));
  const pills = new Map();
  for (const P of F0.pills.values()) {
    if (inFocus(P.parent)) { pills.set(P.id, P); continue; }
    const D = new Map([...P.D].filter(([k]) => star(k))), C = new Map([...P.C].filter(([k]) => star(k)));
    if (D.size || C.size || called.has(P.id)) pills.set(P.id, {...P, D, C});
  }
  const frames = new Map();
  for (const [fid, f] of F0.frames) {
    if (inFocus(fid)) frames.set(fid, {...f, parent: inFocus(f.parent) ? f.parent : null});
    else if (unitAt(fid) && !holds(fid)) frames.set(fid, {...f, parent: unitAt(f.parent) && !holds(f.parent) ? f.parent : REST_FRAME});
  }
  if ([...frames.values()].some(f => f.parent === REST_FRAME)) frames.set(REST_FRAME, {id: REST_FRAME, parent: null});
  const places = new Map([...F0.places].filter(([k]) => star(k)).map(([k, pl]) => [k, "home" in pl ? {...pl, home: inFocus(pl.home) ? pl.home : undefined} : pl]));
  return {...F0, places, pills, frames, calls};
}
// The sets of packages put in focus: each subset while there are few, else each alone and each pair
function focusSets(F0) {
  const units = [...F0.frames.values()].filter(f => f.step.startsWith("unit:")).map(f => f.step.slice(5));
  return units.length <= 4
    ? [...Array(1 << units.length).keys()].slice(1).map(m => units.filter((_, i) => m & (1 << i)))
    : [...units.map(u => [u]), ...units.flatMap((a, i) => units.slice(i + 1).map(b => [a, b]))];
}
// a focus as what it draws, its arrows to and from the rest with it; a picture that cannot be read so says why
function focusDrawn(C) {
  try {
    return drawnOf(C) + JSON.stringify((C.restArrows || []).map(([r, at, side, ts]) => [r, at, side, [...ts.keys()].sort()]).sort());
  } catch (e) {
    return `a picture without what a plan's image carries: ${e.message}`;
  }
}
// The boxes at the border of a package drawn closed, by the way the lines cross it: a type box it is joined to, and a type a
// counted arrow to or from the rest stands for, as the box it is drawn as
function borderOf(C, id) {
  const P = C.pills.get(id), ways = {D: new Set(), C: new Set()};
  if (!P) return null;
  for (const way of ["D", "C"]) for (const k of P[way].keys()) if (k.startsWith("t:")) ways[way].add(k);
  for (const [r, at, side, ts] of C.restArrows || []) {
    const ks = ts instanceof Map ? [...ts.keys()] : [...ts].map(t => "t:" + M.placeOfType(t));
    if (at === id) ks.forEach(k => ways[side === "~in" ? "D" : "C"].add(k));
    if (r === id) ks.forEach(k => ways[side === "~in" ? "C" : "D"].add(k));
  }
  return JSON.stringify([[...ways.D].sort(), [...ways.C].sort()]);
}
// For every set of packages in focus `U` and every set of open boxes (its packages and the rest): the focus's fine plan is the
// fiber of `F₀` restated above (`starOf`); its picture is the image of that plan under `q_U` as the overview's is of `F₀`
// (each line counts the fine lines it stands for and none is reached, a type internal to a closed package draws none, each call
// goes between the blocks of its ends, opening one more box refines the picture, the picture is a function of the set of open
// boxes, and the picture the page draws has both ends of every line among its boxes); a package of the focus drawn closed has
// at its border the type boxes the overview draws at its border, the overview opened as the focus is and at the packages above
// it; and a type box stands in the frame the overview puts it in, where that frame is drawn, else outside every package: a
// type's home is a fact of `F₀`, the same in every view. All of it with the test code and without it
function focusFaults(tests) {
  const out = [], pre = tests ? "" : "without the test code, ";
  const clear = () => { for (const k of Object.keys(V.open)) V.open[k] = new Set(); };
  V.route = null; V.kind = null; V.part = null; V.focus = new Set(); V.tests = tests;
  clear();
  M.plan();
  const F0 = window.atlasPlan.fine;
  for (const U of focusSets(F0)) {
    const name = `${pre}the focus on ${U.join(", ")}`, star = starOf(F0, U);
    const above = new Set();
    for (const [fid, f] of F0.frames) if (U.includes(f.step.slice(5))) for (let g = f.parent; g; g = F0.frames.get(g).parent) above.add(F0.frames.get(g).step);
    const boxes = [...star.frames.values()].map(f => f.step || "unit:~rest");
    const sets = boxes.length <= ALL_SUBSETS
      ? [...Array(1 << boxes.length).keys()].map(m => boxes.filter((_, i) => m & (1 << i)))
      : [[], ...boxes.map(b => [b]), ...boxes.flatMap((a, i) => boxes.slice(i + 1).map(b => [a, b])), boxes];
    const first = new Map();
    for (const set of sets) {
      const at = `${name}${set.length ? `, with ${set.join(", ")} open` : ""}`;
      V.focus = new Set(U); clear(); openAs(set);
      window.atlasPlan = null;
      const C = M.plan(), F = window.atlasPlan?.fine;
      first.set(set, focusDrawn(C));
      // the overview, opened as the focus is and at every package above one in focus
      const own = [...V.open.units];
      V.focus = new Set(); clear(); openAs([...set.filter(s => s !== "unit:~rest"), ...above]);
      const O = M.plan();
      for (const [fid, f] of (F || star).frames) {
        if (!f.step || f.step === "unit:~rest" || f.parent === REST_FRAME || !C.pills.has(fid) || !O.pills.has(fid)) continue;
        const a = borderOf(C, fid), b = borderOf(O, fid);
        if (a !== b) out.push(`${at}: ${fid} has at its border ${a}, in the overview ${b}`);
      }
      for (const c of [...C.places.keys()].filter(k => k.startsWith("t:"))) {
        if (!C.home) { out.push(`${at}: the focus places no box where F₀ puts it`); break; }
        const h = O.home?.([c], false), want = h && C.frames.has(h) ? h : null, got = C.home([c], false) || null;
        if (want !== got) out.push(`${at}: ${c} stands in ${got || "the picture"}, and F₀ puts it in ${want || "the picture"}`);
      }
      V.focus = new Set(U); clear(); V.open.units = new Set(own);
      if (!F || F === F0) { out.push(`${at}: the focus is drawn by a construction of its own, not from F₀`); continue; }
      if (!set.length && planOf(F) !== planOf(star)) out.push(`${name}: the fine plan is not the fiber of F₀ over the focus: ${planOf(F)} against ${planOf(star)}`);
      const view = M.imageOf(F, M.focusOpen, false);
      for (const f of M.checkOpenings(F, view, false, M.focusOpen)) out.push(`${at}: ${f}`);
      const block = id => { let top = null; for (let f = F.pills.get(id).parent; f; f = F.frames.get(f)?.parent) if (!M.focusOpen(F.frames.get(f).step)) top = f; return top || id; };
      const inside = (home, S) => { for (let f = home; f; f = F.frames.get(f)?.parent) if (f === S) return true; return false; };
      const want = new Map();
      for (const P of F.pills.values()) for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const k of m.keys()) {
        const b = block(P.id);
        if (b !== P.id && inside(F.places.get(k)?.home, b)) continue;
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
      for (const f of ends(C, M.coarsen(F, M.focusOpen, () => false))) out.push(`${at}: ${f}`);
    }
    for (const set of [...sets].reverse()) {
      V.focus = new Set(U); clear(); openAs(set);
      if (focusDrawn(M.plan()) !== first.get(set)) out.push(`${name}${set.length ? `, with ${set.join(", ")} open` : ""}: drawn again after the other sets, the picture differs`);
    }
  }
  V.focus = new Set(); clear();
  return out;
}

// All routes of a kind are the union of the routes' coarsest pictures, each route closed (mathematics.md, "One fine plan,
// every picture its image", all routes of a kind): a route's box takes and gives what crosses the border of its route closed,
// the image of its finest plan with no box open, as the route's own page plans it. The routes of a kind have no order between
// them and so no moments: a value is its type, or its name where it has no type of the code's, and what the parse of the command
// line makes is the entry's. Each line counts the fine lines of its route it stands for, and of them those it reaches. Asked of
// every kind of route the program has, on the kind's plan, before the siblings that draw alike are merged. The entries of
// the routes are one family: closed, the kind's picture draws one entry box, into which every route's entry line goes with its
// count; opened, it is the union of the routes' entries, each route's line into its own
function kindFaults() {
  const out = [], kinds = [...new Set([...M.byId.values()].filter(n => n.entry && n.entry.kind !== "test").map(n => n.entry.kind))];
  const clear = () => { V.route = null; V.kind = null; V.part = null; V.focus = new Set(); V.tests = false; for (const k of Object.keys(V.open)) V.open[k] = new Set(); };
  for (const kind of kinds) {
    clear(); V.kind = kind;
    M.plan();
    const K = window.atlasPlan.fine, rs = M.overviewEntries().map(e => e.id);
    out.push(...entryFaults(kind, K, rs));
    for (const r of rs) {
      clear(); V.route = r;
      M.plan();
      const F = window.atlasPlan.fine, foldable = ["cli", "script"].includes(M.byId.get(r).entry.kind);
      const closed = M.imageOf(F, () => false, foldable, {fineKeys: true});
      const asKind = k => {
        const pl = F.places.get(k);
        if (!/^[tv]:/.test(k)) return k;
        if (pl?.at && closed.hidden.has(pl.at)) return "e:" + r;
        return pl?.type ? "t:" + pl.type : k.replace(/@.*$/, "");
      };
      const want = new Map();
      for (const B of closed.pills.values()) for (const [way, n, far] of [["D", B.Dn, B.Dr], ["C", B.Cn, B.Cr]]) for (const k of B[way].keys()) {
        const key = `${way} ${asKind(k)}`, [c, cr] = want.get(key) || [0, 0];
        want.set(key, [c + (n?.get(k) || 1), cr + (far?.get(k) || 0)]);
      }
      const P = K.pills.get("n:" + r), drawn = new Map();
      for (const way of ["D", "C"]) for (const [k, [c, cr]] of P?.mult?.[way] || []) drawn.set(`${way} ${k}`, [c, cr]);
      const say = m => JSON.stringify([...m].sort());
      if (say(drawn) !== say(want)) out.push(`all ${kind} routes: ${r} draws ${say(drawn)}, its route closed ${say(want)}`);
    }
  }
  clear();
  return out;
}

// the entries of a kind's routes, closed and opened, against each route's entry line in the kind's plan
function entryFaults(kind, K, rs) {
  const out = [], fam = "e:~" + kind, entryLines = r => [...(K.pills.get("n:" + r)?.mult?.D || [])].filter(([k]) => k === "e:" + r);
  for (const open of [false, true]) {
    V.route = null; V.kind = kind; V.focus = new Set();
    for (const k of Object.keys(V.open)) V.open[k] = new Set();
    if (open) V.open.places.add(fam);
    const view = M.imageOf(K, () => false, false), at = `all ${kind} routes${open ? ", their entries open" : ""}`;
    const boxes = [...view.places.keys()].filter(k => k.startsWith("e:"));
    const want = rs.length > 1 && !open ? [fam] : rs.filter(r => entryLines(r).length).map(r => "e:" + r);
    if (JSON.stringify(boxes.sort()) !== JSON.stringify([...want].sort())) out.push(`${at}: the entry boxes are ${JSON.stringify(boxes.sort())}, the routes' entries ${JSON.stringify([...want].sort())}`);
    for (const r of rs) for (const [k, [c]] of entryLines(r)) {
      const id = rs.length > 1 && !open ? fam : k, got = view.pills.get("n:" + r)?.Dn?.get(id);
      if (got !== c) out.push(`${at}: ${r} takes ${got ?? 0} from ${id}, its route's entry ${c}`);
    }
  }
  V.open.places = new Set();
  return out;
}

// ---- every view state draws the image of its fine plan ----
// The view state `V` is one value, the one the page's address holds. Its coordinates are what the reader looks at (a route,
// the routes of a kind, one part, the packages in focus, the test code or not), which decide the fine plan, and what the reader
// has open, which decides the map `q`. The law walks that space: for every program, every view (the overview and each part
// alone, each focus, each kind, each route), with the test code and without, at the closed picture, at each box of each
// coordinate opened alone, and at sets of boxes of all coordinates together drawn from a seed. Each picture is held to its
// plan: every line ends at a box it draws (`ends`), each line counts the fine lines it stands for and a box stands where the
// plan puts it (`quotient`, or for a plan of generators `generated`), opening one more box refines it (`checkOpenings`), the
// picture is a function of the view state (drawn again after every other, it is the same) and of the coordinates its view
// reads alone (a coordinate it does not read leaves it as it is)
const COORDINATES = {route: null, kind: null, part: null, focus: "set", tests: "flag", open: {parts: "set", units: "set", steps: "set", functions: "set", families: "set", places: "set", dirs: "set"}};
// the coordinates each view reads; the others leave its picture as it is
const READS = {
  route: ["route", "open.steps", "open.functions", "open.families", "open.places", "open.dirs"],
  kind: ["kind", "part", "open.families", "open.places", "open.dirs"],
  focus: ["focus", "part", "tests", "open.units", "open.families", "open.places", "open.dirs"],
  overview: ["part", "tests", "open.parts", "open.units", "open.families", "open.places", "open.dirs"],
};
const VIEW_SEED = Number(process.env.ATLAS_VIEWS_SEED || 437) >>> 0;
// how many sets of boxes of all coordinates together each view is asked at, and how many boxes it opens alone at most
const VIEW_SETS = Number(process.env.ATLAS_VIEWS_SETS || 4), VIEW_SINGLES = Number(process.env.ATLAS_VIEWS_SINGLES || 12);
// A coordinate of `V` that the law does not draw is a view it cannot see: the keys are read from the page's own view state
function coordinateFaults() {
  const out = [], drawn = Object.keys(COORDINATES).filter(k => k !== "open"), open = Object.keys(COORDINATES.open);
  for (const k of Object.keys(V)) if (k !== "open" && !drawn.includes(k)) out.push(`the view state has a coordinate ${k} that the law does not draw`);
  for (const k of Object.keys(V.open)) if (!open.includes(k)) out.push(`the view state has a coordinate open.${k} that the law does not draw`);
  for (const k of drawn) if (!(k in V)) out.push(`the law draws a coordinate ${k} that the view state does not have`);
  for (const k of open) if (!(k in V.open)) out.push(`the law draws a coordinate open.${k} that the view state does not have`);
  for (const r of Object.values(READS)) for (const c of r) if (!(c.startsWith("open.") ? open.includes(c.slice(5)) : drawn.includes(c))) out.push(`a view reads ${c}, no coordinate of the law`);
  return out;
}
const blank = () => ({route: null, kind: null, part: null, focus: new Set(), tests: false, open: Object.fromEntries(Object.keys(V.open).map(k => [k, new Set()]))});
const setState = S => {
  V.route = S.route; V.kind = S.kind; V.part = S.part; V.focus = new Set(S.focus); V.tests = S.tests;
  for (const k of Object.keys(V.open)) V.open[k] = new Set(S.open[k] || []);
};
const nameState = S => {
  const o = {};
  for (const k of ["route", "kind", "part"]) if (S[k]) o[k] = S[k];
  if (S.focus.size) o.focus = [...S.focus].sort();
  if (S.tests) o.tests = true;
  for (const [k, v] of Object.entries(S.open)) if (v.size) (o.open ||= {})[k] = [...v].sort();
  return JSON.stringify(o);
};
// a picture as what the page is handed of it: its boxes and frames, the lines of each with their counts, the side of each
// place, the calls, the rest of a focus, the order of the disk
function pictureOf(C) {
  try {
    return focusDrawn(C) + JSON.stringify([[...(C.meets?.frames.keys() || [])].sort(), [...(C.fiber || [])].sort()]);
  } catch (e) {
    return `a picture without what a plan's image carries: ${e.message}`;
  }
}
// The boxes each coordinate of `V.open` can hold in a view, read off its plan: the boxes of the code's hierarchy and the steps of
// a route that its plan frames, the functions those run, the families of the types it holds, the groups of its places, the
// entry of a route that parses its command line and the entries of a kind's routes, the directories of its files and the places
// on the disk that hold others
function boxesOf(F, view) {
  const frames = [...F.frames.values()].filter(f => f.step && !f.synthetic && !f.anchor), steps = frames.map(f => f.step);
  const types = [...F.places.values()].filter(p => p.type).flatMap(p => [M.familyOf(p.type), M.kindOf(p.type)]);
  const files = [...F.places.keys()].map(M.filePath).filter(Boolean);
  const entry = view === "route" && ["cli", "script"].includes(M.byId.get(V.route)?.entry?.kind) ? ["e:" + V.route] : [...(F.entries?.keys() || [])];
  return {
    parts: steps.filter(s => s.startsWith("part:")).map(s => s.slice(5)),
    units: steps.filter(s => s.startsWith("unit:")).map(s => s.slice(5)),
    steps: view === "route" ? steps : [],
    functions: view === "route" ? [...new Set(steps.map(M.stepOf))].filter(s => M.byId.has(s)) : [],
    families: [...new Set(types)].filter(t => M.opens(t)).sort(),
    places: [...new Set([...entry, ...[...F.places.values()].map(p => p.group).filter(Boolean)])].sort(),
    dirs: [...new Set([...files.flatMap(M.dirsOf), ...Object.values(D.flow?.lies_in || {}).flat().map(q => q.slice(7))])].sort(),
  };
}
// The image of a plan of generators (the overview, a focus, the routes of a kind), as the definition takes it: an occurrence
// goes to its outermost closed frame, a type whose home lies in a closed block is internal to it, each other line counts the
// fine lines it stands for and of them those it reaches, and a call goes between the blocks of its ends, internal to one that
// holds both; a type box stands in the frame its home is, where that frame is drawn
function generated(F, view, opened) {
  const out = [];
  const block = id => { let top = null; for (let f = F.pills.get(id).parent; f; f = F.frames.get(f)?.parent) if (!opened(F.frames.get(f).step)) top = f; return top || id; };
  const inside = (home, S) => { for (let f = home; f; f = F.frames.get(f)?.parent) if (f === S) return true; return false; };
  const want = new Map();
  for (const P of F.pills.values()) for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const [k, r] of m) {
    const b = block(P.id);
    if (b !== P.id && inside(F.places.get(k)?.home, b)) continue;
    const key = JSON.stringify([b, way, view.q(k)]), [n, nr] = P.mult?.[way]?.get(k) || [1, r ? 1 : 0], e = want.get(key) || [0, 0];
    want.set(key, [e[0] + n, e[1] + nr]);
  }
  const drawn = new Map();
  for (const B of view.pills.values()) for (const [way, n, r] of [["D", B.Dn, B.Dr], ["C", B.Cn, B.Cr]]) for (const [c, k] of n || []) drawn.set(JSON.stringify([B.id, way, c]), [k, r?.get(c) || 0]);
  for (const [k, [n, nr]] of want) if (JSON.stringify(drawn.get(k)) !== JSON.stringify([n, nr])) out.push(`the line ${k} counts ${JSON.stringify(drawn.get(k) ?? null)}, the fine lines ${JSON.stringify([n, nr])}`);
  for (const k of drawn.keys()) if (!want.has(k)) out.push(`the line ${k} stands for no fine line`);
  const calls = new Set((F.calls || []).map(([a, b]) => [block("n:" + a), block("n:" + b)]).filter(([x, y]) => x !== y).map(([x, y]) => `${x}>${y}`));
  const drawnCalls = new Set([...view.pills.values()].flatMap(P => [...(P.calls || [])].map(b => `${P.id}>${b}`)));
  for (const c of calls) if (!drawnCalls.has(c)) out.push(`no call ${c}`);
  for (const c of drawnCalls) if (!calls.has(c)) out.push(`the call ${c} stands for no call of the code`);
  // a type of an open family stands in the family's frame, which stands where the family's home is
  const homes = new Map(), boxOf = k => { const c = view.q(k), p = view.places.get(c), f = p?.type && M.frameOf(p.type); return f ? M.valueBox(f, p) : c; };
  for (const [k, pl] of F.places) if ("home" in pl && view.places.has(view.q(k))) { const c = boxOf(k); if (!homes.has(c)) homes.set(c, new Set()); homes.get(c).add(pl.home || null); }
  for (const [c, hs] of homes) if (hs.size === 1) {
    const h = [...hs][0], want = h && view.frames.has(h) ? h : null, got = view.homeOf([c], false) || null;
    if (h && !view.frames.has(h) && F.frames.has(h)) out.push(`${c} is drawn, and its home ${h} is a closed box that holds every step that makes or takes it`);
    else if (want !== got) out.push(`${c} stands in ${got || "the picture"}, and its plan puts it in ${want || "the picture"}`);
  }
  return out;
}
// A route ends at a place (sentence 3): a box that stands for a route closed gives something, a value, a place it changes or a
// call it makes. A route whose code gives nothing and changes nothing is a finding of the program, said as one
function endsFaults(name, F, C) {
  const out = [], routes = new Set(M.overviewEntries().map(e => e.id));
  const gives = B => !!B && (B.C.size > 0 || (B.calls?.size || 0) > 0 || !!B.backTo);
  if (V.route) {
    const folded = ["cli", "script"].includes(M.byId.get(V.route)?.entry?.kind);
    const A = M.imageOf(F, () => false, folded), B = A.pills.get("n:" + V.route) || A.pills.get("o:" + V.route);
    if (!gives(B)) out.push(`${name}: the route ${V.route} ends at no place`);
  } else if (V.kind) for (const B of C.pills.values()) for (const r of B.steps || []) if (routes.has(r) && !gives(B)) out.push(`${name}: the route ${r} ends at no place`);
  return out;
}
// every route of the program closed, alone and among the routes of its kind, ends at a place
function routeEnds() {
  const out = [];
  for (const kind of [...new Set([...M.byId.values()].filter(n => n.entry).map(n => n.entry.kind))].sort()) {
    setState({...blank(), kind});
    const C = M.plan();
    out.push(...endsFaults(`kind ${kind}`, window.atlasPlan.fine, C));
  }
  for (const n of [...M.byId.values()].filter(n => n.entry)) {
    setState({...blank(), route: n.id});
    const C = M.plan();
    out.push(...endsFaults(`route ${n.id}`, window.atlasPlan.fine, C));
  }
  setState(blank());
  return out;
}
// the faults of one picture, held to its plan
function stateFaults(name, S) {
  setState(S);
  window.atlasPlan = null;
  const C = M.plan(), {fine: F, opened} = window.atlasPlan || {}, out = [], at = f => out.push(`${name}: ${f}`);
  if (!F || !opened) { at("the picture is not the image of a plan"); return {out, C}; }
  const view = M.viewOf(), {coarsest} = M.VIEWS[view];
  const folded = view === "route" && ["cli", "script"].includes(M.byId.get(V.route)?.entry?.kind) && !V.open.places.has("e:" + V.route);
  const image = M.imageOf(F, opened, folded);
  for (const f of ends(C, M.coarsen(F, opened, coarsest))) at(f);
  setState(S);
  for (const f of M.checkOpenings(F, image, folded, opened)) at(f);
  setState(S);
  if (view === "route") {
    const {occ, valueName, values} = namesOf(F), homes = homesOf(F, values), set = [...V.open.steps, ...[...F.frames.values()].filter(f => f.step && !f.synthetic && V.open.functions.has(M.stepOf(f.step))).map(f => f.step)];
    for (const f of quotient(F, image, set, homes, occ, valueName)) at(f);
  } else for (const f of generated(F, image, opened)) at(f);
  setState(S);
  return {out, C: M.plan()};
}
// a set of each coordinate's boxes, drawn: each box with a chance the set draws first
const drawSet = (draw, xs) => { const p = draw(); return xs.filter(() => draw() < p); };
function viewFaults() {
  const out = coordinateFaults(), draw = random(VIEW_SEED);
  const kinds = [...new Set([...M.byId.values()].filter(n => n.entry).map(n => n.entry.kind))].sort();
  const parts = (D.parts || []).length > 1 ? D.parts.map(p => p.id) : [];
  // the views: what the reader looks at, each a state with nothing open
  const bases = [];
  for (const tests of [false, true]) {
    for (const part of [null, ...parts]) bases.push({...blank(), part, tests});
    for (const kind of kinds) for (const part of [null, ...parts]) bases.push({...blank(), kind, part, tests});
  }
  setState(blank()); M.plan();
  const F0 = window.atlasPlan.fine, units = [...F0.frames.values()].filter(f => f.step.startsWith("unit:")).map(f => f.step.slice(5));
  for (const tests of [false, true]) for (const U of units.length <= 3 ? [...Array(1 << units.length).keys()].slice(1).map(m => units.filter((_, i) => m & (1 << i))) : [...units.map(u => [u]), ...[...Array(VIEW_SETS).keys()].map(() => drawSet(draw, units)).filter(u => u.length)]) bases.push({...blank(), focus: new Set(U), tests});
  for (const n of [...M.byId.values()].filter(n => n.entry)) bases.push({...blank(), route: n.id});
  const seen = new Map();
  for (const base of bases) {
    setState(base); M.plan();
    const view = M.viewOf(), F = window.atlasPlan.fine, boxes = boxesOf(F, view), reads = new Set(READS[view]);
    const coords = Object.keys(boxes).filter(k => reads.has("open." + k) && boxes[k].length);
    const states = [base];
    const singles = coords.flatMap(k => boxes[k].map(b => [k, b]));
    for (const [k, b] of singles.length <= VIEW_SINGLES ? singles : [...Array(VIEW_SINGLES).keys()].map(() => singles[Math.floor(draw() * singles.length)])) states.push({...base, open: {...base.open, [k]: new Set([b])}});
    for (let i = 0; i < VIEW_SETS && coords.length; i++) states.push({...base, open: {...base.open, ...Object.fromEntries(coords.map(k => [k, new Set(drawSet(draw, boxes[k]))]))}});
    for (const S of states) {
      const name = `${view} ${nameState(S)}`;
      if (seen.has(name)) continue;
      const {out: faults, C} = stateFaults(name, S);
      out.push(...faults);
      const picture = pictureOf(C);
      seen.set(name, [S, picture]);
      // a coordinate the view does not read leaves the picture as it is: one below it in what decides the view (a route before
      // a kind, a kind before a focus), the part and the test code where it does not read them, and an open box of another view
      const other = {...S, open: {...S.open}};
      if (view === "route" && kinds.length) other.kind = kinds[0];
      if ((view === "route" || view === "kind") && units.length) other.focus = new Set([units[0]]);
      if (!reads.has("part") && parts.length) other.part = parts[0];
      if (!reads.has("tests")) other.tests = !S.tests;
      for (const k of Object.keys(other.open)) if (!reads.has("open." + k)) other.open[k] = new Set(["unread"]);
      setState(other);
      const again = pictureOf(M.plan());
      if (again !== picture) out.push(`${name}: drawn with ${nameState(other)}, coordinates the view does not read, the picture differs`);
    }
  }
  // the picture is a function of the view state: drawn again after all the others, each is the picture it was
  for (const [name, [S, picture]] of [...seen].reverse()) {
    setState(S);
    if (pictureOf(M.plan()) !== picture) out.push(`${name}: drawn again after the other states, the picture differs`);
  }
  setState(blank());
  return out.map(f => `seed ${VIEW_SEED}: ${f}`);
}

// with no route named, every route of the program but its tests; `--overview` asks the overview, with the test code and without;
// `--focus` every focus, with the test code and without; `--kinds` the routes of each kind; `--views` every view state; `--ends`
// whether every route ends at a place
// the answer is written whole before the process ends: a pipe takes some 64 KB at a time
const said = answer => process.stdout.write(JSON.stringify(answer), () => process.exit(0));
if (routes[0] === "--views") said({"@views": {facts: [], faults: viewFaults()}});
else if (routes[0] === "--ends") said({"@ends": {facts: [], faults: routeEnds()}});
else if (routes[0] === "--kinds") said({"@kinds": {facts: [], faults: kindFaults()}});
else if (routes[0] === "--focus") said({"@focus": {facts: [], faults: [...focusFaults(true), ...focusFaults(false)]}});
else if (routes[0] === "--overview") {
  const fiber = testFaults();
  said({"@overview": {facts: fiber.facts, faults: [...overviewFaults(true), ...overviewFaults(false), ...fiber.faults]}});
} else {
  const named = routes.length ? routes : [...M.byId.values()].filter(n => n.entry && n.entry.kind !== "test").map(n => n.id);
  const result = {};
  for (const r of named) {
    if (Date.now() > DEADLINE) break;
    result[r] = read(r);
  }
  process.stdout.write(JSON.stringify(result));
}
