// The model of a page: the graph it is handed, read as routes, and the finest plan of a route, every picture of it
// its image under the quotient the reader has chosen (docs/reference/mathematics.md, "One fine plan, every picture its
// image"). It is the body of a function of `D`, the page's data, and `window` (`ATLAS_FINE_BUDGET`, `ATLAS_ORACLE`,
// `atlasPlan`): the page inlines it into its own script, where its names are the page's, and `engine/tests/plans.rs`
// runs it in node on small programs whose plans are written down. It draws nothing and touches no document.

// two readings drawn as one, when the page is their union (`--since`, `atlas history`): what B
// added, removed and changed, stamped on the nodes, steps, touches and edges of the union
const DIFF = D.diff || null;
if (DIFF) {
  for (const n of D.nodes) {
    n.ds = DIFF.nodes[n.id] || null;
    (n.sites || []).forEach((s, i) => { s.ds = DIFF.sites[n.id]?.[i] || null; });
    [...(n.touches || []), ...(n.instead || [])].forEach((t, i) => { t.ds = DIFF.touches[n.id]?.[i] || null; });
  }
  D.edges.forEach((e, i) => { e.ds = DIFF.edges[i] || null; });
}
// the reading a picture is drawn from: A ("a") holds what B removed, B ("b") what it added, and
// both hold the rest; null is the page's graph as it is
let SIDE = null;
const inReading = (x, s) => !x || !x.ds || x.ds === "changed" || x.ds === (s === "a" ? "removed" : "added");
const has = x => !SIDE || inReading(x, SIDE);
const edgesBy = new Map();
for (const e of D.edges) { const k = `${e.src}>${e.dst}>${e.kind}`; if (!edgesBy.has(k)) edgesBy.set(k, []); edgesBy.get(k).push(e); }
const edgeOn = (src, dst, kind) => { const es = edgesBy.get(`${src}>${dst}>${kind}`); return !es || es.some(has); };
// a node by the name it has in the reading drawn: one B renamed has its old name in A
const nameOf = n => SIDE === "a" && DIFF?.renamed[n.id] ? DIFF.renamed[n.id].split(".").pop() : n.name;
// a step of a body as the reading drawn has it: in A, a step both readings hold is handed what A hands it
const siteRead = (n, i) => { const s = n?.sites?.[i], h = SIDE === "a" && DIFF?.hands[n.id]?.[i]; return h ? {...s, hands: h} : s; };
// the sums of the code by their variants: a value of `A | B` is one value, whatever the reader lists of it
const SUMS = new Map();
{
  const vs = new Map();
  for (const e of D.edges) if (e.kind === "variant") { if (!vs.has(e.src)) vs.set(e.src, new Set()); vs.get(e.src).add(e.dst); }
  for (const [sum, v] of vs) SUMS.set([...v].sort().join("|"), sum);
}
// the sum a variant is injected into, where it is the variant of one
const SUM_OF = new Map(), VARIANTS = new Map();
for (const e of D.edges) if (e.kind === "variant") { SUM_OF.set(e.dst, SUM_OF.has(e.dst) && SUM_OF.get(e.dst) !== e.src ? null : e.src); if (!VARIANTS.has(e.src)) VARIANTS.set(e.src, []); VARIANTS.get(e.src).push(e.dst); }
// whether a parameter's written type names a type: by its name, or a sum written in place by the names of its variants
// (`r: Fit | Failed`)
const writesType = (p, x) => { const w = new Set((p.type || "").split(/[^A-Za-z0-9_]+/)), b = id => id.replace(/^ext:/, "").split(".").pop(); return w.has(b(x)) || (VARIANTS.has(x) && VARIANTS.get(x).every(v => w.has(b(v)))); };
// the values a call is handed and gives, one value per name: the variants of a sum a name holds are the sum, a value
// of no type a constructor gives is of the class it builds, one a change gives is the value it was handed, and one
// handed by a name of no type is the value
// an earlier call of the body gave that name
const sitesCache = new Map();
const sitesOf = n => {
  const k = `${SIDE}|${n.id}`;
  if (sitesCache.has(k)) return sitesCache.get(k);
  const sum = hs => {
    const by = new Map();
    for (const h of hs || []) if (h.name && h.type) { if (!by.has(h.name)) by.set(h.name, new Set()); by.get(h.name).add(h.type); }
    const out = [], done = new Set();
    for (const h of hs || []) {
      const ts = h.name && by.get(h.name), s = ts && ts.size > 1 && SUMS.get([...ts].sort().join("|"));
      if (!s) out.push(h);
      else if (!done.has(h.name)) { done.add(h.name); out.push({...h, type: s}); }
    }
    return out;
  };
  const named = new Map();
  const sites = (n.sites || []).map((_, i) => {
    const s = siteRead(n, i);
    if (!s) return s;
    const one = byId.get(s.to)?.kind === "class" ? s.to : (s.to && arrowOf(s.to)?.builds) || "";
    const hands = sum(s.hands).map(h => (!h.type && h.name && named.has(h.name) ? {...h, type: named.get(h.name)} : h));
    // a call that changes a value it is handed gives it anew under its name: of the type it was handed as
    const was = new Map(hands.filter(h => h.name && h.type).map(h => [h.name, h.type]));
    const gives = sum(s.gives).map(h => (!h.type && h.name && (was.get(h.name) || one) ? {...h, type: was.get(h.name) || one} : h));
    for (const g of gives) if (g.name && g.type) named.set(g.name, g.type);
    return s.hands || s.gives ? {...s, hands, gives} : s;
  });
  sitesCache.set(k, sites);
  return sites;
};
const siteIn = (n, i) => (n?.sites?.[i] ? sitesOf(n)[i] : undefined);

const byId = new Map(D.nodes.map(n => [n.id, n]));
// a prophecy is sketch code not built yet, and the members of one stand with it
const prophecyOf = n => n?.prophecy || (n?.cls && byId.get(n.cls)?.prophecy) || null;
// what a prophecy replaces, and the prophecy that replaces a node
const replacedBy = new Map(D.nodes.filter(n => n.prophecy?.replaces).map(n => [n.prophecy.replaces, n.id]));
// the units that are Makefiles, each one of its part
const MAKEFILES = new Set((D.make || []).map(t => t.module));
for (const t of (D.make || [])) byId.set(t.id, {id: t.id, kind: "make", name: t.label, module: t.module, part: t.part, doc: t.doc, recipe: t.recipe, entry: {kind: "make", label: t.label}, make: t});
const ROOT = D.root + ".";
// a repository of several parts (a crate, a package, an app), each read in its own language
const PARTS = D.parts || [], multi = PARTS.length > 1;
const partOf = n => n.part || (PARTS[0] || {}).id || D.root;
const langOf = n => (PARTS.find(p => p.id === partOf(n)) || {}).language || D.language;
const partRank = n => Math.max(0, PARTS.findIndex(p => p.id === partOf(n)));
// what the picture shows, as data: a view. Its fibers keep the part of the code the reader looks at
// (one route, the routes of one kind, one part, the packages in focus, with the rest of the code
// quotiented to either side); its opened sets are order ideals of the code's hierarchies, each the
// refinement of a quotient: parts into packages, packages into subpackages, steps into their bodies
// (one call by its path through the route, or every call of a function), families into their
// members, coarse places into their parts, directories into their files
const V = {route: null, kind: null, part: null, focus: new Set(), open: {parts: new Set(), units: new Set(), steps: new Set(), functions: new Set(), families: new Set(), places: new Set(), dirs: new Set()}};
const short = id => id.startsWith(ROOT) ? id.slice(ROOT.length) : id.replace(/^ext:/, "");
const KG = {imports:"imports", calls:"calls", requests:"calls", instantiates:"calls", takes:"types", returns:"types", has:"types", mutates:"types", variant:"types", inherits:"inherits"};

// ---------- indexes ----------
// a library read as code is drawn where a route steps into it, not as code of the project
const OWN = D.nodes.filter(n => !n.library);
const modules = OWN.filter(n => n.kind === "module");
const symsOf = new Map();  // module id -> [class/function nodes]
for (const n of OWN) if (n.kind === "class" || n.kind === "function") { if (!symsOf.has(n.module)) symsOf.set(n.module, []); symsOf.get(n.module).push(n); }
const out = new Map(), inn = new Map();
for (const e of D.edges) { if (!out.has(e.src)) out.set(e.src, []); out.get(e.src).push(e); if (!inn.has(e.dst)) inn.set(e.dst, []); inn.get(e.dst).push(e); }
const modOf = n => n.kind === "module" ? n.id : n.module;
// a box takes the hue of its package: the first level of the code quotiented by module path, the
// level a part's overview shows; what lies outside the code has no package. The legend names the
// packages the picture shows.
let HUES = null, shownUnits = new Set();
const unitCache = new Map();
function unitOfNode(n) {
  if (!n || n.kind === "external" || n.kind === "make" || !n.module) return null;
  if (!unitCache.has(n.id)) unitCache.set(n.id, topUnits(partOf(n)).find(u => inUnit(n.id, u)) ?? null);
  return unitCache.get(n.id);
}
function hueOf(n) {
  if (!HUES) { HUES = new Map(); for (const p of PARTS.length ? PARTS.map(p => p.id) : [partOf({})]) for (const u of topUnits(p)) if (!MAKEFILES.has(u) && !HUES.has(u)) HUES.set(u, `var(--hue-${(HUES.size % 8) + 1})`); }
  const u = unitOfNode(n);
  if (u) shownUnits.add(u);
  return HUES.get(u) || "var(--external)";
}

// ---------- routes: entry points, their steps, and the world they touch ----------
// where a touch rests, which side of its step it stands on and what it does there: the engine says
const isIn = t => t.side === "in";
// the code does not say which way the data goes
const isOpen = t => t.side === "open";
// which way a step goes at each place it touches, by the place the engine names, and whether what it
// reads there goes on to what it hands on (a read that only checks does not): what makes a place's
// shape goes no way
const go = (goes, t) => {
  if (t.shape) return;
  const k = t.place.id;
  if (!goes.has(k)) goes.set(k, new Set());
  goes.get(k).add(isOpen(t) ? "open" : isIn(t) ? "read" : "write");
  if (isIn(t) && !t.checks) goes.get(k).add("use");
};
const joinGoes = (into, from) => from?.forEach((s, k) => { if (!into.has(k)) into.set(k, new Set()); s.forEach(x => into.get(k).add(x)); });
const verbOf = t => t.verb;
const KIND_LABEL = {make: "Make", cli: "CLI", script: "Script", route: "API", mcp: "MCP", notebook: "Notebook", export: "Library", prophecy: "Prophecy", test: "Test"}, KIND_ORDER = ["make", "cli", "export", "script", "notebook", "route", "mcp", "prophecy", "test"];
const entries = [...byId.values()].filter(n => n.entry).sort((a, b) => (partRank(a) - partRank(b)) || (KIND_ORDER.indexOf(a.entry.kind) - KIND_ORDER.indexOf(b.entry.kind)) || a.entry.label.localeCompare(b.entry.label));
// what running a function means, from the engine's reading: its body on a value of class `on` (the
// calls in the order they run, a method called on a known class that class's), or on any value
const FLOW = (D.flow || {}).bodies || {}, ARROWS = (D.flow || {}).arrows || {};
// where each touch is drawn, as the flow says: the place it rests in, the side of its step, what it does there
for (const n of D.nodes) [...(n.touches || []), ...(n.instead || [])].forEach((t, i) => Object.assign(t, ((D.flow || {}).drawn || {})[n.id]?.[i]));
// the places a coarse place stands for until the picture splits its group: the disk for the files
// only a variable names, a database for its tables
const COARSE = new Map();
for (const n of D.nodes) for (const t of [...(n.touches || []), ...(n.instead || [])]) if (t.place?.coarse) {
  const c = t.place.coarse.id;
  if (!COARSE.has(c)) COARSE.set(c, new Map());
  COARSE.get(c).set(t.place.id, t.place);
}
// in one reading of a union, a body runs the steps that reading has
const bodyCache = new Map();
const bodyOf = (id, on) => {
  const k = on ? `${id}@${on}` : id, b = FLOW[k] || {items: []};
  if (!SIDE) return b;
  const ck = `${SIDE}|${k}`;
  if (!bodyCache.has(ck)) {
    const n = byId.get(id), runs = s => has(byId.get(s.to)) && (s.site == null || has(n?.sites?.[s.site]));
    const items = b.items.flatMap(it => { if (!it.members) return runs(it) ? [it] : []; const ms = it.members.filter(runs); return ms.length ? [{...it, members: ms}] : []; });
    bodyCache.set(ck, {...b, items, alts: (b.alts || []).filter(a => has(byId.get(a)))});
  }
  return bodyCache.get(ck);
};
// what a function takes and gives in the reading drawn: the arrows of the union that reading has
const arrowCache = new Map();
const arrowOf = id => {
  const a = ARROWS[id], n = byId.get(id);
  if (!a || !SIDE || !n || n.kind === "class") return a;
  const ck = `${SIDE}|${id}`;
  if (!arrowCache.has(ck)) arrowCache.set(ck, {...a, takes: (a.takes || []).filter(x => edgeOn(id, x, "takes")), gives: (a.gives || []).filter(x => edgeOn(id, x, "returns")), changes: (a.changes || []).filter(x => edgeOn(id, x, "mutates"))});
  return arrowCache.get(ck);
};
const stepsIn = b => b.items.flatMap(it => it.members ? it.members : [it]);
// what a call may run in place of the function it reaches: a method called by name on a value whose class is not
// known, any override its body lists; one whose call names the method (`Cached.read(self)`), what its call says
const insteadOf = s => s.named ? (s.instead || []).filter(a => has(byId.get(a))) : s.on ? [] : bodyOf(s.to, null).alts || [];
// the functions a step runs: its body's steps, and what the call may run instead (`alts`, else the overrides of a call by name)
const callees = (id, on, alts = null) => { const b = bodyOf(id, on); return [...new Set([...stepsIn(b).map(s => s.to), ...(alts ?? b.alts ?? [])])].filter(t => byId.has(t)); };
// what each step of an opened route stands for: the function it runs (or a library's operation,
// `op:to_numpy`), the class of the value it runs on, and the call; a step is a call, keyed by its
// path through the route, so two calls of one function are two steps; `fed` names the values of
// its body that come from the route's entry
const treeInfo = new Map();
// whether the opened route parses its command line in a step, which opening its entry shows
let routeParses = false;
const stepOf = key => treeInfo.get(key)?.step ?? key;
// a constructor, whatever the language calls it: the function that builds its class; its
// initializer is what a call to the class runs, the one named after the class
const isCtor = n => !!(n && arrowOf(n.id)?.builds);
const initializer = c => (byId.get(c)?.methods || []).map(m => `${c}.${m}`).find(id => byId.get(id)?.ctor && !byId.get(id).on_class);
// what building a class runs: the initializer along its order, its own or a base's; a class built from its fields (a Rust
// struct expression) runs nothing
const runsOf = c => arrowOf(c)?.runs || null;
const isInit = id => !!byId.get(id)?.ctor && !byId.get(id).on_class;
// everything a step touches, anywhere below it: the touches of every function its body runs, on the
// class each runs on, and of everything each call may run instead; `alts` is what the call into `id` may
// run instead, where it says (`insteadOf`), else the overrides of a call by name
const reachCache = new Map();
function reach(id, on, root = null, chain = null, alts = null) {
  const k0 = on ? `${id}@${on}` : id, kc = `${SIDE}|${k0}#${root || ""}${chain ? "|" + [...chain].sort() : ""}${alts ? "!" + alts : ""}`;
  if (reachCache.has(kc)) return reachCache.get(kc);
  // a call that names its method is kept apart from a call by name of the same function: they run different alternatives
  const seen = [], vis = new Set([k0 + (alts ? "!" : "")]), stack = [[id, on, alts]];
  while (stack.length) {
    const [cur, con, calts] = stack.pop(), n = byId.get(cur); if (!n) continue;
    seen.push(n);
    const b = bodyOf(cur, con);
    for (let [t, o, a] of [...stepsIn(b).map(s => [s.to, s.on || null, s.named ? insteadOf(s) : null]), ...(calts ?? b.alts ?? []).map(x => [x, null, null])]) {
      if (byId.get(t)?.kind === "class") { t = runsOf(t); o = null; a = null; }
      const k = (o ? `${t}@${o}` : t) + (a ? "!" : "");
      if (byId.has(t) && !vis.has(k)) { vis.add(k); stack.push([t, o, a]); }
    }
  }
  const nodes = new Set([...vis].map(k => k.replace(/!$/, "").split("@")[0]));
  // under a step of a route, a caller that makes a path whole is one on the calls that lead to the step, or one it reaches itself
  const path = chain && new Set([...chain, ...nodes]), touches = new Map();
  for (const n of seen) for (const t of touchesIn(n, root, path)) { const k = touchKey(t); if (!touches.has(k)) touches.set(k, t); }
  const r = {touches: [...touches.values()], nodes}; reachCache.set(kc, r); return r;
}
// a route shows the places its own calls hand: a touch whose path or statement a caller makes
// whole, when the route reaches that caller, and under a step of it, when the caller is on the
// calls that lead there (`path`); one an entry's users hand, in that entry's route; and a call
// none of whose touches it runs, as the call was read. A method runs on an instance its class's
// constructor built, before the route or in it, so a value the constructor holds (`self.path`) is
// whole wherever a method of its class runs
const builtFor = (g, seen) => {
  const c = byId.get(g);
  return !!(c?.ctor && c.cls) && [...seen].some(f => { const m = byId.get(f); return m && !m.ctor && m.cls === c.cls; });
};
const inRoute = (t, root, path) => {
  if (!root || (!(t.by || []).length && !(t.users_of || []).length) || (t.users_of || []).includes(root)) return true;
  const seen = path || reach(root, null).nodes;
  return (t.by || []).some(f => f.split("&").every(g => seen.has(g) || builtFor(g, seen)));
};
function touchesIn(n, root, path) {
  const own = (n.touches || []).filter(has);
  if (!root) return own;
  const runs = own.filter(t => inRoute(t, root, path));
  return runs.concat((n.instead || []).filter(has).filter(t => !runs.some(o => o.line === t.line && o.op === t.op && o.kind === t.kind)));
}
const touchKey = t => [t.kind, t.what, t.artifact || "", t.dir || "", t.op || ""].join("|");
// the routes an overview shows: all of them, or those of one kind (all Library routes, all CLI routes)
const overviewEntries = () => entries.filter(e => has(e) && (V.kind ? e.entry.kind === V.kind : e.entry.kind !== "test") && (!V.part || partOf(e) === V.part));
// in a union, a route as the diff marks it: added or removed with its entry, changed where a step
// it runs, a step's steps or a place they touch is
const routeMark = e => {
  if (!DIFF) return "";
  if (e.ds === "added" || e.ds === "removed") return e.ds === "added" ? "+ " : "− ";
  return [...reach(e.id, null).nodes].some(id => byId.get(id)?.ds || DIFF.sites[id] || DIFF.touches[id]) ? "~ " : "";
};

// ---------- plaat: a route as places and steps ----------
// The fiber of a route (an entry and the steps opened in it): every step a pill between what it takes and what it hands on, then
// quotient types into their families. Zooming in is nesting: an opened step is a frame around its
// own steps, an opened family a frame around its subclasses or parts, and a call into a family is a
// choice between its members. A closed step stands for everything below it, and what it touches
// down there is dashed: a step's own morphism and what it reaches are different quantities. The
// world stands still at the edges: what the route only reads on the left, what it only writes on
// the right.
// a family is rooted in a base the program uses as a type: some signature takes or returns it, a
// field holds it, a sum names it. A base that only shares code (a config mixin, pydantic's
// BaseModel, torch's Module, a derived trait) is how a class is built, not what flows
// the engine groups the types: a subclass stands in its base's place (Python and Rust are kinds of
// Language), a part in its one owner's, and a type holds what other types hold too as a reference
const TYPES = (D.flow || {}).types || {};
const familyOf = id => (TYPES.family || {})[id] || id;
const kindOf = id => (TYPES.kind || {})[id] || id;
const members = new Map(Object.entries(TYPES.members || {}));
const sharedOf = f => ((TYPES.shared || {})[f] || []).map(s => ({via: s.via, dst: s.to}));
// a route draws its types on its own fiber, the types its finest plan holds: the kinds of a type are those of them the fiber
// holds (fiber first, then the quotient), so a route draws the same whatever other routes do. Outside a route the fiber is
// the whole program
let FIBER = null;
const here = t => !FIBER || FIBER.has(t);
const kindsOf = t => (members.get(t) || []).filter(m => kindOf(m) === t && here(m));
const partsOf = t => (members.get(t) || []).filter(m => kindOf(m) !== t);
// a kind is a box of its own where the fiber holds two of its types
const folds = k => kindsOf(k).length + (here(k) ? 1 : 0) > 1;
// a type opens when it has kinds or parts of its own, or holds what it shares; (n) counts the types it opens into
const opens = t => folds(t) || partsOf(t).length > 0 || sharedOf(t).length > 0;
const opensInto = t => kindsOf(t).length + partsOf(t).length + sharedOf(t).length + (here(t) ? 1 : 0);
const bare = id => byId.has(id) ? nameOf(byId.get(id)) : id.replace(/^ext:/, "").split(".").pop();
const nameCount = new Map(); for (const n of D.nodes) if (n.kind === "class" || n.kind === "external") nameCount.set(bare(n.id), (nameCount.get(bare(n.id)) || 0) + 1);
const typeLabel = id => { const b = bare(id); if ((nameCount.get(b) || 0) > 1) { const mod = (byId.get(id)?.module || id.replace(/^ext:/, "").split(".").slice(0, -1).join(".")).split(".").pop(); return mod ? `${mod}.${b}` : b; } return b; };
const entryTitle = id => { const n = byId.get(id); return n.entry ? `${KIND_LABEL[n.entry.kind]} ${n.entry.label}` : short(id); };
// what a step changes in place is in its codomain too: `T -> T`

// ---------- layers: the code quotiented by module path, one level at a time ----------
// a unit is a module path: a package with everything under it, or a package's own module by itself
// (`app.serve$`). A part's top level is the children of its own package, and each tree beside it
// (scripts, notebooks) as one; a level with a single child is passed through
const inUnit = (id, u) => { const m = byId.get(id)?.module ?? ""; return u.endsWith("$") ? m === u.slice(0, -1) : m === u || m.startsWith(u + "."); };
// the units of each level, as the engine reads them
const UNITS = (D.flow || {}).units || {tops: [], kids: {}};
const unitsBelow = u => UNITS.kids[u] || [];
const topUnits = part => (UNITS.tops.find(([p]) => p === part) || [null, []])[1];
const unitLabel = u => { if (u === "~in" || u === "~out") { const n = aroundFocus().length; return `the rest · ${n} ${n === 1 ? "package" : "packages"}`; } if (u.startsWith("~")) return unitLabel(u.slice(4)); const base = u.replace(/\$$/, ""), last = base === "_root" ? "scripts at the root" : base.split(".").pop(); return u.endsWith("$") ? `${last} itself` : last; };
const partOfUnit = u => { const m = [...byId.values()].find(n => n.kind === "module" && inUnit(n.id, u)); return m ? partOf(m) : partOf({}); };
const inFocus = id => [...V.focus].some(u => inUnit(id, u));
// the rest of the code around a focus: `~in` hands data to it, `~out` takes data from it; opened, each
// is the packages it holds (`~in:app.serve`)
const isRest = u => u.startsWith("~");
// the packages of the rest that hand the focus data (`~in`) or take data from it (`~out`), each with the types
function restExchange(side) {
  const out = new Map(), around = aroundFocus();
  for (const [t, takers] of usesOf().takes) {
    const makers = usesOf().makes.get(t);
    if (!makers || !isData(t)) continue;
    const [from, into] = side === "~in" ? [makers, takers] : [takers, makers];
    if (![...into].some(inFocus)) continue;
    for (const p of around) if ([...from].some(x => inUnit(x, p))) { if (!out.has(p)) out.set(p, new Set()); out.get(p).add(t); }
  }
  return out;
}
// the packages of the focus's parts that are not in it, nor around or inside one that is
function aroundFocus() {
  const parts = new Set([...V.focus].map(partOfUnit));
  return [...parts].flatMap(p => topUnits(p)).filter(u => !MAKEFILES.has(u) && ![...V.focus].some(f => { const a = f.replace(/\$$/, ""), b = u.replace(/\$$/, ""); return a === b || a.startsWith(b + ".") || b.startsWith(a + "."); }));
}
// the packages around a focus: those that hand data to it or take data from it, the most first
function neighbours() {
  const cands = aroundFocus();
  const crossing = c => { let k = 0; for (const [t, takers] of usesOf().takes) { const makers = usesOf().makes.get(t); if (!makers || !isData(t)) continue; const tk = [...takers], mk = [...makers]; if ((mk.some(x => inUnit(x, c)) && tk.some(inFocus)) || (mk.some(inFocus) && tk.some(x => inUnit(x, c)))) k++; } return k; };
  return cands.map(u => ({u, n: crossing(u)})).filter(x => x.n).sort((a, b) => b.n - a.n || a.u.localeCompare(b.u));
}
// a unit's routes: its entries, then its doors, what other units call in it
function unitRoutes(u) {
  const es = overviewEntries().filter(e => inUnit(e.id, u)).map(e => e.id), seen = new Set(es);
  const called = n => (inn.get(n.id) || []).some(e => (e.kind === "calls" || e.kind === "instantiates") && !inUnit(e.src, u));
  const doors = [...byId.values()].filter(n => has(n) && ["function", "method", "class"].includes(n.kind) && inUnit(n.id, u) && !seen.has(n.id) && called(n)).map(n => n.id).sort();
  return [...es, ...doors];
}
// a class whose own operations reach a database, the web, a model or a program (a sink, a
// repository, a client) is a door to that place, not a place where data rests: between units the
// place is drawn, not the door; the engine marks it (`door`), and a family is a door when one of
// its members is. So is a library's type whose calls reach a place (`sqlite3.Connection`); what
// crosses between units is data
const doors = new Set(TYPES.doors || []);
const isDoor = t => doors.has(t);
const isData = t => { const n = byId.get(t); return (n?.kind === "class" || n?.kind === "external") && !isDoor(t); };
// who makes each type (gives it back, builds it, changes it) and who takes it: a type crosses between
// two units when one makes it and the other takes it. In one reading of a union, by the edges that
// reading holds, as the engine reads them for the union
const typeUse = { makes: new Map((TYPES.makes || []).map(([t, by]) => [t, new Set(by)])), takes: new Map((TYPES.takes || []).map(([t, by]) => [t, new Set(by)])) };
const MAKES = new Set(["returns", "instantiates", "mutates"]), usesIn = new Map();
const usesOf = () => {
  if (!SIDE) return typeUse;
  if (!usesIn.has(SIDE)) {
    const u = {makes: new Map(), takes: new Map()};
    for (const e of D.edges) {
      const m = e.kind === "takes" ? u.takes : MAKES.has(e.kind) ? u.makes : null;
      if (!m || byId.get(e.src)?.library || !inReading(e, SIDE)) continue;
      if (!m.has(e.dst)) m.set(e.dst, new Set());
      m.get(e.dst).add(e.src);
    }
    usesIn.set(SIDE, u);
  }
  return usesIn.get(SIDE);
};
// where a route comes from: the command typed, the request made, the tool called
const ENTRY_PLACE = {cli: l => `$ ${l}`, make: l => `$ make ${l}`, script: l => `$ python ${l}`, notebook: l => `notebook ${l}`, route: l => l, mcp: l => `MCP ${l}`, export: (l, n) => `${n ? partOf(n) : D.root}.${l}`, prophecy: l => `prophecy ${l}`, test: l => `test ${l}`};
// an entry's domain is what the human hands it on the command line: it reads as its usage line,
// `$ sunspots --raw --figure --months`
// in A, an entry whose usage line B changed reads as A had it
const entryLabel = n => { const e = SIDE === "a" && DIFF?.entries[n.id] || n.entry; return ENTRY_PLACE[e.kind](e.label, n) + (e.options || []).map(o => " " + o).join(""); };
const entryPlace = id => ({id: "e:" + id, label: entryLabel(byId.get(id)), kind: "world"});
// the steps of a body that belong to its entry: the parses of the command line (`Cli::parse()`,
// `parseArgs(..)`) and the steps whose values only those parses take (the parser argparse is
// handed); and the names of what the parses give, which come from the entry
function commandLine(n) {
  const sites = (n?.sites || []).map((_, i) => siteIn(n, i)), parses = sites.map((s, i) => s.command_line && has(s) ? i : -1).filter(i => i >= 0);
  const hidden = new Set(parses), fed = new Set(parses.flatMap(i => (sites[i].gives || []).map(g => g.name)));
  const takers = name => sites.flatMap((s, i) => has(s) && (s.hands || []).some(h => h.name === name) ? [i] : []);
  sites.forEach((s, i) => { if (has(s) && !hidden.has(i) && (s.gives || []).length && s.gives.every(g => g.name && takers(g.name).length && takers(g.name).every(t => parses.includes(t)))) hidden.add(i); });
  return {hidden, fed};
}

// ---------- the disk is a tree: files fold into their directories ----------
// the files of a picture, each standing in the directory it is folded into, and how many stand in each
let dirFold = {to: new Map(), n: new Map()};
const filePath = id => id.startsWith("w:File:") ? id.slice(7) : null;
// the directories a path is in, from the top: `a/b/c.csv` -> `a/`, `a/b/`
const dirsOf = p => { const segs = p.replace(/\/$/, "").split("/"), out = []; for (let i = 1; i < segs.length; i++) out.push(segs.slice(0, i).join("/") + "/"); return out.filter(d => d !== "/"); };
// a picture with more files than a column holds folds them: each file stands in the highest
// directory that holds more than one of them and is not opened, taken as deep as it holds the
// same files (`artefacts/*/` when everything in `artefacts/` is under `*`)
function foldDirs(paths) {
  const to = new Map(), n = new Map();
  if (paths.length <= 6) return {to, n};
  const holds = d => paths.filter(p => p.startsWith(d)).length;
  for (const p of paths) {
    const ds = dirsOf(p);
    let at = p;
    for (let i = 0; i < ds.length; i++) {
      if (V.open.dirs.has(ds[i])) continue;
      const k = holds(ds[i]);
      if (k < 2) break;
      let j = i; while (j + 1 < ds.length && !V.open.dirs.has(ds[j + 1]) && holds(ds[j + 1]) === k) j++;
      at = ds[j]; break;
    }
    if (at !== p) { to.set(p, at); n.set(at, holds(at)); }
  }
  return {to, n};
}
const filePlace = path => {
  const d = dirFold.to.get(path) || path, k = dirFold.n.get(d);
  return k ? {id: "w:File:" + d, label: `File(${d}) · ${k}`, short: `${d} · ${k}`, kind: "world", folded: d} : {id: "w:File:" + path, label: `File(${path})`, short: path, kind: "world"};
};
// a figure made is seen where a notebook runs the cell that made it: on a notebook's route it is
// on the screen; elsewhere it rests in the figure the library holds, which only `show` puts there
const shownAs = t => t.kind === "figure" && byId.get(V.route)?.entry?.kind === "notebook" ? {...t, kind: "screen", side: "out", place: {id: "w:Screen", label: "Screen", short: "Screen", glyph: "screen"}} : t;
// a part of the world as the picture draws it now: the place the engine names, its coarser place
// until the reader splits that group, a file in the directory it is folded into
const placeGlyph = new Map();
let worldShown = new Map();
function worldPlaceOf(t, fine = false) {
  let p = t.place;
  if (!fine && worldShown.has(p.id)) p = worldShown.get(p.id);
  const q = p.file ? filePlace(p.file) : {id: p.id, label: p.label, short: p.short, kind: "world", coarse: p.coarse, group: p.group};
  placeGlyph.set(q.id, p.glyph);
  return q;
}
// the route as a tree: an open step holds its own steps; a call into a family holds its members' overrides as branches
// in one reading of a union, a package or a part is there while code of it is
const heldCache = new Map();
const held = (key, test) => { const k = `${SIDE}|${key}`; if (!heldCache.has(k)) heldCache.set(k, [...byId.values()].some(n => n.kind === "module" && test(n) && has(n))); return heldCache.get(k); };
// the finest plan of a route opens every call: what the reader has open is a quotient of it, taken afterwards (`coarsen`)
let FINE = false;
// a route can unfold into more occurrences than a plan can hold (calls shared by many callers): the finest plan opens every call
// down to the depth that keeps it within a budget, and what the reader has open at any depth; a call it does not open is a
// block of its own, with what it reaches
// a test sets a small budget (`window.ATLAS_FINE_BUDGET`) to cut the plan of a small program
const FINE_BUDGET = window.ATLAS_FINE_BUDGET || 5000;
let FINE_DEPTH = Infinity, fineCount = 0, fineCut = false;
// a step beyond the cut, unfolded by itself: its body one level deep, its own calls cut in turn (`cutArrow`)
let unfoldStep = null;
function routeTree() {
  const closedRoutes = ids => ids.map(id => ({id, open: false, kids: [], hue: null}));
  if (V.route && !has(byId.get(V.route))) return [];
  if (!V.route && !V.kind) {
    const unitNodes = (us, part) => us.filter(u => !SIDE || MAKEFILES.has(u) || held("u:" + u, n => inUnit(n.id, u))).map(u => { const open = V.open.units.has(u); return {id: "unit:" + u, open, kids: open ? unitKids(u, part) : [], hue: null, choice: false}; });
    const unitKids = (u, part) => { const below = u.endsWith("$") || MAKEFILES.has(u) ? [] : unitsBelow(u, part); return below.length > 1 ? unitNodes(below, part) : closedRoutes(unitRoutes(u)); };
    // a focus: the packages chosen, each opened as before, and the rest of the code on either side
    if (V.focus.size) {
      const rest = r => { const open = V.open.units.has(r); return {id: "unit:" + r, open, kids: open ? [...restExchange(r).keys()].sort().map(p => ({id: `unit:${r}:${p}`, open: false, kids: [], hue: null, choice: false})) : [], hue: null, choice: false}; };
      return [...[...V.focus].sort().flatMap(u => unitNodes([u], partOfUnit(u))), rest("~in"), rest("~out")];
    }
    if (multi && !V.part) return PARTS.filter(p => !SIDE || held("p:" + p.id, n => partOf(n) === p.id)).map(p => { const open = V.open.parts.has(p.id); return {id: "part:" + p.id, open, kids: open ? unitNodes(topUnits(p.id), p.id) : [], hue: null, choice: false}; });
    const part = V.part || partOf({});
    return unitNodes(topUnits(part), part);
  }
  if (!V.route) return closedRoutes(overviewEntries().map(e => e.id));
  treeInfo.clear();
  // a route run from the command line starts at what the human typed: the parse of the command
  // line belongs to the entry, and what it gives comes from the entry, until the entry is opened
  const foldable = ["cli", "script"].includes(byId.get(V.route)?.entry?.kind), folds = !FINE && foldable && !V.open.places.has("e:" + V.route);
  routeParses = false;
  // what a step runs is its body, as the engine reads it: what the call that runs it may run instead (`by.alts`, else
  // the overrides of a call by name), then its own steps, each choice's branches in a frame, each branch in its own colour
  const visit = (key, step, on, site, hue, path, fed, by = {}) => {
    const alts = by.alts ?? bodyOf(step, on).alts ?? [];
    treeInfo.set(key, {step, on, site, fed, path, named: !!by.named, alts, from: by.from, siteNo: by.siteNo, dispatch: by.dispatch});
    const n = byId.get(step), forced = V.open.steps.has(key) || V.open.functions.has(step);
    // the class a step builds: the class its call names, or the class a route starts at for its initializer door; an
    // initializer called by another (`super(text)`) builds nothing, it fills in the value its caller builds
    treeInfo.get(key).built = by.built ?? (key !== V.route && by.from === V.route && libraryClass(V.route) && isInit(step) ? V.route : undefined);
    fineCount++;
    // a step that builds a class runs its initializer (`call`); a class built from its fields runs nothing, and its
    // methods are what a route of its own starts at
    if (key !== V.route && n?.kind === "class") return {id: key, open: false, kids: [], hue, choice: false};
    const open = key === V.route || forced || (FINE && path.size < FINE_DEPTH && fineCount <= 4 * FINE_BUDGET), node = {id: key, open, kids: [], hue, choice: false};
    if (!open || !n) {
      if (FINE && n && (callees(step, on, alts).length || stepsIn(bodyOf(step, on)).length)) { node.cut = true; fineCut = true; }
      return node;
    }
    const deeper = new Set([...path, step]), body = bodyOf(step, on);
    node.choice = alts.length > 0;
    let b = 0;
    for (const o of alts) node.kids.push(visit(`${key}|${o}`, o, null, null, (b++ % 8) + 1, deeper, undefined, {dispatch: key}));
    const parse = commandLine(n);
    if (parse.hidden.size) routeParses = true;
    const entry = folds ? parse : {hidden: new Set(), fed: new Set()};
    const call = (s, h) => {
      if (entry.hidden.has(s.site)) return null;
      // a call to a function the route is already inside is a step of the route that stays closed: the frame it would open is the one it
      // stands in, and the border it crosses is drawn
      // building a class runs its initializer, and building it is no call of the class a route starts at
      const built = byId.get(s.to)?.kind === "class" ? s.to : null, to = built ? runsOf(built) || built : s.to;
      if (deeper.has(to) && to !== built) {
        const rk = `${key}/${s.site ?? s.to}`;
        treeInfo.set(rk, {step: to, on: s.on || null, site: s.site != null ? siteIn(n, s.site) : null, fed: entry.fed, path: deeper, recursive: true, named: !!s.named, alts: insteadOf(s), from: key, siteNo: s.site, built});
        return {id: rk, open: false, kids: [], hue: h, choice: false, recursive: true};
      }
      const k = visit(`${key}/${s.site ?? s.to}`, to, s.on || null, s.site != null ? siteIn(n, s.site) : null, h, deeper, entry.fed, {named: s.named, alts: insteadOf(s), from: key, siteNo: s.site, built});
      // in the finest plan the steps that only parse the command line are there, marked: a closed entry is their quotient
      if (FINE && foldable && parse.hidden.has(s.site)) k.parse = true;
      return k;
    };
    for (const it of body.items) {
      if (!it.members) { const k = call(it, hue); if (k) node.kids.push(k); continue; }
      // the arm of the choice a member stands in, as its call says (`arms`): a choice nested in an arm is drawn in it
      const armIn = m => (m.site != null && (siteIn(n, m.site)?.arms || []).find(([c]) => c === it.choice)?.[1]) ?? m.branch;
      const kids = it.members.map(m => { const k = call(m, m.branch % 8 + 1); if (k) k.branch = armIn(m); return k; }).filter(Boolean);
      // what decides each branch, in the branch's colour; a choice of one branch runs it only when that holds
      const branches = [...new Set(it.members.map(m => m.branch))].sort((a, b) => a - b);
      const when = branches.map(b => [b % 8 + 1, (it.when || [])[b] || ""]);
      if (kids.length) node.kids.push({id: `${key}~c${it.choice}`, open: true, synthetic: true, choice: true, kids, hue, when});
    }
    return node;
  };
  if (!FINE) return [visit(V.route, V.route, null, null, null, new Set())];
  unfoldStep = key => {
    const i = treeInfo.get(key), kept = [FINE_DEPTH, fineCount, fineCut, routeParses];
    FINE_DEPTH = i.path.size + 1; fineCount = 0;
    try { return visit(key, i.step, i.on, i.site, null, i.path, i.fed, i); } finally { [FINE_DEPTH, fineCount, fineCut, routeParses] = kept; }
  };
  let tree;
  const unfold = depth => { FINE_DEPTH = depth; fineCount = 0; fineCut = false; treeInfo.clear(); routeParses = false; tree = [visit(V.route, V.route, null, null, null, new Set())]; };
  let depth = 1;
  for (unfold(depth); fineCut; unfold(++depth)) if (fineCount > FINE_BUDGET) { unfold(Math.max(1, depth - 1)); break; }
  return tree;
}

// the subject of a route stands as itself; other types stand in their family until it is opened
// a closed family holds its kinds: a type stands in its kind until that family is opened; a part of a type is structure of
// its owner, drawn in its frame when opened, and the values of its type stand on their own
const placeOfType = t => t === V.route || V.open.families.has(familyOf(t)) || V.open.families.has(kindOf(t)) || !folds(kindOf(t)) ? t : kindOf(t);
// the route's subject stands as itself, in no family
const frameOf = t => t === V.route ? undefined : [kindOf(t), t].find(f => V.open.families.has(f) && opens(f) && (f === t || folds(f)));
const typePlace = t => {
  const n = byId.get(t), closedFam = t !== V.route && opens(t) && !V.open.families.has(t);
  return {id: "t:" + t, label: typeLabel(t) + (closedFam ? ` (${opensInto(t)})` : ""), kind: n && n.kind === "external" ? "ext" : "type", hue: hueOf(n), type: t};
};
const stepName = id => { if (id.startsWith("part:")) { const p = PARTS.find(x => x.id === id.slice(5)) || {}; return `${p.id} · ${p.language}`; } if (id.startsWith("unit:")) return unitLabel(id.slice(5)); const n = byId.get(id); if (!V.route && n.entry) return entryLabel(n); if (n.kind === "class") return nameOf(n) + "()"; if (n.make) return n.name; if (isCtor(n) && !n.on_class) return typeLabel(n.cls) + "()"; return n.cls ? `${typeLabel(n.cls)}.${nameOf(n)}` : nameOf(n); };
// a step's name: a method run on a subclass is that class's, one its call names is its own class's, a library's
// operation is its own name
const stepLabel = key => {
  const info = treeInfo.get(key);
  if (!info) return stepName(key);
  if (info.step.startsWith("op:")) return info.step.slice(3);
  if (info.built) return typeLabel(info.built) + "()";
  const n = byId.get(info.step);
  return info.on && !info.named && n && n.cls && !isCtor(n) ? `${typeLabel(info.on)}.${nameOf(n)}` : stepName(info.step);
};
// what the call at a step of the route may run instead, where its call says so (a call that names its method)
const callAlts = key => (treeInfo.get(key)?.named ? treeInfo.get(key).alts : null);
const openable = key => { const s = stepOf(key), on = treeInfo.get(key)?.on; return key !== V.route && !treeInfo.get(key)?.recursive && !V.open.steps.has(key) && !V.open.functions.has(s) && !s.startsWith("op:") && byId.get(s)?.kind !== "class" && callees(s, on, callAlts(key)).length + stepsIn(bodyOf(s, on)).length > 0; };
// an exported class has no body of its own: its route starts at each public method the caller can call
const libraryClass = id => byId.get(id)?.kind === "class" && !!byId.get(id).entry;
// a door of the class a route starts at: called by the route's caller, with no order between the doors
const isDoorOf = key => !!V.route && treeInfo.get(key)?.from === V.route && libraryClass(V.route);
const momentOf = id => id.includes("@") ? id.slice(id.indexOf("@")) : "";
// what a route finds, updates and touches, read once for every plan of it
const routeContent = new Map();
// one reading's picture before it is laid out: its places, its pills and frames, each value at its
// moments, and in a focus the arrows from and to the rest of the code
const planBase = (fine, tree = null) => {
// `tree`: the plan of one step beyond the cut by itself (`cutArrow`), which does not know the values of its callers
const alone = !!tree;
const places = new Map();   // id -> {id, label, kind: world | type | ext, hue, fam}
const restLinks = new Map();  // in a focus: unit -> the types it takes from the rest and hands to it
const addPlace = p => { if (!places.has(p.id)) places.set(p.id, p); return p.id; };
const worldPlace = t => worldPlaceOf(t, true);
// the routes drawn, and what each finds where it reads: what was there before it ran, not what it
// wrote itself
const anchors = V.route ? [V.route] : overviewEntries().map(e => e.id), seen = `${SIDE}|${V.route}|${V.kind}|${fine}`;
if (!routeContent.has(seen)) {
  const found = new Set(), updated = new Set(), content = {places: new Set(), files: new Set(), dbs: new Set()};
  for (const a of anchors) {
    const back = new Set(arrowOf(a)?.reads_back || []);
    // the route of a class is its doors, each called by the entry with no order between them: a place one of them finds
    // and changes, the route finds and changes
    const doors = byId.get(a)?.kind === "class" ? stepsIn(bodyOf(a, null)).map(s => s.to).filter(Boolean) : [];
    for (const f of [a, ...doors]) for (const p of arrowOf(f)?.updates || []) updated.add(p);
    for (const t of reach(a, null, a).touches) {
      if (t.shape || isOpen(t)) continue;
      content.places.add(t.place.id);
      if (t.place.file) content.files.add(t.place.file);
      if (t.kind.startsWith("db_")) content.dbs.add(t.kind);
      if (isIn(t) && !back.has(t.place.id)) found.add(t.place.id);
    }
  }
  routeContent.set(seen, {found, updated, content});
}
const {found, updated, content} = routeContent.get(seen);
// a place's shape (a directory made, a table created) and a connection opened without a way serve
// the data the route reads or writes there: of a file, in a directory; of a database, any table
const inDir = dir => [...content.files].some(f => f.startsWith(dir));
const served = t => (t.shape || isOpen(t)) && (t.kind.startsWith("db_") ? content.dbs.has(t.kind) : !!t.place.file && (content.places.has(t.place.id) || (t.place.file.endsWith("/") && inDir(t.place.file))));
// a type is a place of its own while its moments are counted; then a closed one stands in its kind.
// The kind is the type's (its base class), not the view's: the route's subject is its own
const kindKey = k => k.startsWith("t:") && places.get(k)?.type ? "t:" + (places.get(k).type === V.route ? V.route : kindOf(places.get(k).type)) : k;
// a step's morphism: its own body, or (closed) its interface and everything it touches below
// noReceiver: the step is seen from its own class (a library class's route), so the receiver is not an input
const morphism = (id, own, isAnchor, noReceiver, opts = {}) => {
  if (id.startsWith("part:")) return partMorphism(id.slice(5), own);
  if (id.startsWith("unit:")) return unitMorphism(id.slice(5), own);
  const f = byId.get(id), dom = [], cod = [], goes = new Map();  // [placeId, reach]
  if (!f) return {dom: new Map(), cod: new Map(), goes};
  if (isAnchor && f.entry) dom.push([addPlace(entryPlace(id)), false]);
  const merge = r => { r.dom.forEach((v, k) => dom.push([k, v])); r.cod.forEach((v, k) => cod.push([k, v])); joinGoes(goes, r.goes); };
  if (f.kind === "class" && f.entry && !V.route) {
    // a library class among all routes: what it is built from and what its public methods take and hand on,
    // together; inside a route, building one is a step like any constructor, handing on what it builds
    // seen from inside the class, getting the class back (`add` returns self to chain) is not an output
    if (!own) { const self = "t:" + f.id; for (const m of callees(f.id)) { const r = morphism(m, false, false, true); r.cod.delete(self); merge(r); } }
  } else {
    // what the engine says the function takes and gives; the receiver is the class the call
    // says it runs on, when it says; seen from its own class (noReceiver), the value it runs on
    // and builds is no input or output
    const a = arrowOf(f.id) || {};
    const recv = a.receiver ? opts.on || a.receiver : null;
    if (recv && byId.has(recv) && !noReceiver) dom.push([addPlace(typePlace(recv)), false]);
    for (const x of a.takes || []) dom.push([addPlace(typePlace(x)), false]);
    for (const x of a.gives || []) cod.push([addPlace(typePlace(x)), false]);
    if (a.builds && !noReceiver && !opts.fills) cod.push([addPlace(typePlace(opts.built || a.builds)), false]);
    // a method that changes its own class changes the value it runs on
    for (const x of a.changes || []) if (!((noReceiver || a.builds) && x === f.cls)) cod.push([addPlace(typePlace(x === f.cls && recv ? recv : x)), false]);
    // an open step's body makes what it gives back from what its steps make
    if (own && a.gives_back && !opts.leaf) for (const x of bodyOf(f.id, opts.on).made_from || []) dom.push([addPlace(typePlace(x)), false]);
    // a closed call into a family takes and hands on what any of its branches does, unless the call
    // says which class it runs on
    if (!own) for (const o of opts.alts ?? (opts.on ? [] : bodyOf(f.id, null).alts || [])) merge(morphism(o, false, false, noReceiver, {at: opts.at}));
  }
  // a step of a route runs the touches its own calls lead to: the functions on the calls to it, and itself
  const root = V.route || (f.entry ? f.id : null), at = opts.at && treeInfo.get(opts.at), chain = V.route && at?.path ? new Set([...at.path, f.id]) : null;
  const ownKeys = new Set(touchesIn(f, root, chain).map(touchKey));
  // what the step reads back of what it wrote is no input: the place is in its codomain only
  const back = new Set((own ? arrowOf(f.id)?.own_reads_back : arrowOf(f.id)?.reads_back) || []);
  for (const raw of (own ? touchesIn(f, root, chain) : reach(f.id, opts.on, root, chain, opts.alts).touches)) {
    const tc = shownAs(raw);
    if (served(raw) || (isIn(raw) && !raw.shape && back.has(raw.place.id))) continue;
    go(goes, tc);
    // each call that makes a figure makes one: a figure of its own on the screen
    const p = raw.kind === "figure" && tc.kind === "screen" && opts.figure ? figurePlace(opts.figure, opts.label) : worldPlace(tc);
    (isIn(tc) ? dom : cod).push([addPlace(p), !ownKeys.has(touchKey(raw))]);
  }
  const sig = arr => { const m = new Map(); for (const [p, r] of arr) m.set(p, (m.has(p) ? m.get(p) : true) && r); return m; };
  return {dom: sig(dom), cod: sig(cod), goes};
};
// a part: the routes it serves over HTTP come in through its own place, the routes of other
// parts it requests go out to theirs; closed, it also reaches whatever its routes reach
const httpPlace = p => ({id: "w:Web:http:" + p, label: `HTTP(${p})`, short: `HTTP(${p})`, kind: "world"});
const partMorphism = (p, own) => {
  const dom = new Map(), cod = new Map(), goes = new Map();
  if (entries.some(e => has(e) && partOf(e) === p && e.entry.kind === "route" && /^[A-Z|]+ \//.test(e.entry.label))) dom.set(addPlace(httpPlace(p)), false);
  for (const e of D.edges) if (e.kind === "requests" && has(e)) {
    const a = partOf(byId.get(e.src) || {}), b = partOf(byId.get(e.dst) || {});
    if (a === p && b !== p) cod.set(addPlace(httpPlace(b)), false);
  }
  if (!own) for (const e of overviewEntries().filter(e => partOf(e) === p)) {
    const r = morphism(e.id, false, false);
    r.dom.forEach((v, k) => { if (!k.startsWith("t:") && !dom.has(k)) dom.set(k, true); });
    r.cod.forEach((v, k) => { if (!k.startsWith("t:") && !cod.has(k)) cod.set(k, true); });
    joinGoes(goes, r.goes);
  }
  return {dom, cod, goes};
};
// a unit, closed: the places its own code touches (not what it reaches through other units), and
// the types that cross its edge: in where it takes what another unit makes, out where it makes what
// another unit takes
const unitMorphism = (u, own) => {
  const dom = new Map(), cod = new Map(), goes = new Map();
  if (own) return {dom, cod, goes};
  // the rest of the code, seen from a focus, is joined to it by counted arrows, not by boxes
  if (isRest(u)) return {dom, cod, goes};
  for (const n of byId.values()) if (inUnit(n.id, u) && has(n)) {
    const back = new Set(arrowOf(n.id)?.own_reads_back || []);
    for (const tc of (n.touches || []).filter(has)) {
      if (served(tc) || (isIn(tc) && !tc.shape && back.has(tc.place.id))) continue;
      go(goes, tc);
      (isIn(tc) ? dom : cod).set(addPlace(worldPlace(tc)), false);
    }
  }
  // a type crossing the unit's edge is a joint drawn as a box; in a focus, only when another unit in
  // the focus is on its other side, and an arrow from or to the rest otherwise
  const inside = id => inUnit(id, u), theirs = xs => [...(xs || [])].filter(x => !inside(x) && byId.has(x));
  const side = (xs, put, way, t) => {
    const other = theirs(xs);
    if (!other.length) return;
    if (!V.focus.size || other.some(inFocus)) { put.set(addPlace(typePlace(t)), false); return; }
    if (!restLinks.has(u)) restLinks.set(u, {in: new Set(), out: new Set()});
    restLinks.get(u)[way].add(t);
  };
  for (const [t, takers] of usesOf().takes) if (isData(t) && [...takers].some(inside)) side(usesOf().makes.get(t), dom, "in", t);
  for (const [t, makers] of usesOf().makes) if (isData(t) && [...makers].some(inside)) side(usesOf().takes.get(t), cod, "out", t);
  return {dom, cod, goes};
};
// a result with no place of its own (a str, a dict) goes back to the step that called it
// a value it changes in place is no result: the step's result of no type goes back beside it (`rename(board, ..)`
// returns a count)
const handsBack = (id, cod) => {
  if (/^(part|unit|op):/.test(id) || !arrowOf(id)?.returns_value) return false;
  const a = arrowOf(id), gives = new Set([...(a.gives || []), ...(a.builds ? [a.builds] : [])]), changed = t => (a.changes || []).includes(t) && !gives.has(t);
  return ![...cod.keys()].some(p => p.startsWith("t:") && !changed(places.get(p)?.type));
};
// a result another step of its caller's body takes goes on to that step as a value, not back to the caller
const takenBeside = key => {
  const i = treeInfo.get(key), c = i?.from != null && treeInfo.get(i.from), n = c && byId.get(c.step);
  if (!i?.site || !n) return false;
  const names = new Set((i.site.gives || []).filter(g => g.name && !g.type).map(g => g.name));
  return names.size > 0 && (n.sites || []).some((_, j) => j !== i.siteNo && (siteIn(n, j)?.hands || []).some(h => !h.type && names.has(h.name)));
};
// a result the caller drops (`void remove(..)`, a call standing as a statement, the reader's `dropped`) goes nowhere
const goesBack = (key, cod) => handsBack(stepOf(key), cod) && !takenBeside(key) && !treeInfo.get(key)?.site?.dropped;
// a result of no type of the code's that goes back is that one line, labelled by the name that holds it: no value beside it
const backName = (key, cod) => {
  const ns = (treeInfo.get(key)?.site?.gives || []).filter(g => g.name && !g.type).map(g => g.name);
  for (const nm of ns) cod.delete("v:" + nm);
  return ns.find(nm => !nm.startsWith("#")) || null;
};

// a figure a call makes, numbered in the order the route makes them
const figureNo = new Map();
const figurePlace = (key, label) => {
  if (!figureNo.has(key)) figureNo.set(key, figureNo.size + 1);
  const name = `Figure ${figureNo.get(key)} · ${label}`;
  return {id: "w:Screen:fig:" + key, label: name, short: name, kind: "world"};
};
// a step of an opened route is a call: what its function takes and gives, with what the call
// hands it and gets back, each value by the name that holds it; a library's operation has only those
const stepMorphism = (key, own, isAnchor, leaf) => {
  const info = treeInfo.get(key);
  if (!info) return morphism(key, own, isAnchor);
  const {step, on, site, fed} = info, dn = new Map(), cn = new Map(), beyond = new Set(), keep = new Set(), kept = new Set();
  const m = step.startsWith("op:") ? {dom: new Map(), cod: new Map(), goes: new Map()} : morphism(step, own, isAnchor, false, {on, alts: callAlts(key), at: key, figure: key, label: stepLabel(key), leaf, built: info.built, fills: !info.built && isInit(step)});
  // what an opened body gives back, made from what its steps make, is a value of its own, never one of its inputs
  // and what it changes of a value it was handed goes out as what its steps made of it (`other.cells.clear()`)
  const made = new Set(own && !leaf && !step.startsWith("op:") ? [...(arrowOf(step)?.gives_back ? bodyOf(step, on).made_from || [] : []), ...(arrowOf(step)?.changes || [])] : []);
  for (const x of made) keep.add(addPlace(typePlace(x)));
  // an error is a value in the codomain `T + E` (#138): what a site the reader marks `raises` gives leaves the body as an
  // error, and a site marked `fails` hands on its callee's error of that type ("" for no type of the code's). An opened body
  // takes the errors its steps hand on and hands each kind on at its own moment, as its call does closed, up to the route
  const errKey = t => (t ? addPlace(typePlace(t)) : addPlace({id: "v:#error", label: "·", kind: "type", hue: hueOf(null), type: null, name: "#error"}));
  const body = byId.get(step), inner = new Set(), errs = new Set(), raised = new Set();
  if (own && !leaf && !step.startsWith("op:")) (body?.sites || []).forEach((_, i) => { const r = siteIn(body, i); if (r?.raises) for (const g of r.gives || []) inner.add(g.type || ""); if (r?.fails != null) inner.add(r.fails); });
  for (const t of inner) { const k = errKey(t); m.cod.set(k, false); errs.add(k); }
  if (site?.fails != null) { const k = errKey(site.fails); m.cod.set(k, false); errs.add(k); }
  // the route of an exported class is its doors, each called by the entry: a door runs on the caller's instance, one value
  // of the class from beyond, and takes its parameters as a route does
  const door = isDoorOf(key);
  if (door && own) { const r = arrowOf(step)?.receiver; if (r) beyond.add(addPlace(typePlace(on || r))); }
  if (!site && own && (key === V.route || door)) {
    // a route's parameters are values with their names, there from its start: `report(c: Circle, q: Square)`
    // takes `Circle · c` and `Square · q`, two values, whatever family they stand in
    const f = byId.get(step), a = arrowOf(step) || {};
    for (const p of (f?.params || []).filter(p => !p.receiver)) {
      for (const x of (a.takes || []).filter(x => writesType(p, x))) {
        // the parameters of two doors are two values, each door called by itself: `door` tells them apart
        const k = addPlace({...typePlace(x), id: `t:${x}~${p.name}` + (door ? "^" + key : ""), name: p.name, ...(door ? {door: key} : {})});
        if (!made.has(x)) m.dom.delete("t:" + x);
        m.dom.set(k, false); dn.set(k, p.name); beyond.add(k);
      }
    }
  }
  if (site) {
    // a value whose type ty does not know is still the value its name holds; a value no name holds
    // (`#3`, one call's result handed straight to the next) is drawn without one
    // two values of one type a call hands on are two values, each by the name that holds it
    const held = h => addPlace(h.type ? (h.name ? {...typePlace(h.type), id: `t:${h.type}~${h.name}`, name: h.name} : typePlace(h.type)) : {id: "v:" + h.name, label: h.name.startsWith("#") ? "·" : h.name, kind: "type", hue: hueOf(null), type: null, name: h.name});
    // a name the module keeps (`df` under `global df`): where no step of the body made its value, it holds what rests in
    // that place, which the body reads itself (its own touch), no value from beyond
    const body = info.from && treeInfo.get(info.from)?.step;
    const globals = new Set((byId.get(body)?.touches || []).filter(t => t.kind === "global").map(t => t.what));
    for (const h of site.hands || []) {
      // a value the call hands by no name (the value a method runs on, `self.cells.extend(..)`) is a value all the same, not
      // the signature's type that one of a name stands for
      // what the command line gives comes from the entry, in place of the value it is parsed into
      if (fed?.has(h.name)) { if (h.type) m.dom.delete("t:" + h.type); m.dom.set(addPlace(entryPlace(V.route)), false); continue; }
      const k = held(h); if (!m.dom.has(k)) m.dom.set(k, false); if (h.name) dn.set(k, h.name); else keep.add(k);
      if (h.name && globals.has(h.name)) kept.add(k);
    }
    // the parse of the command line, shown where the entry is opened, reads what the human typed
    if (site.command_line && ["cli", "script"].includes(byId.get(V.route)?.entry?.kind)) m.dom.set(addPlace(entryPlace(V.route)), false);
    for (const h of site.gives || []) { const k = held(h); if (!m.cod.has(k)) m.cod.set(k, false); if (h.name) cn.set(k, h.name); }
    // a call is handed the values the names in its arguments hold: where it hands values and none of a parameter's kind,
    // the call fills that parameter from what it hands (a projection, `prefix(settings.level)`, glued into the call; a
    // value of a type no reader knows), and the parameter is no value of its own. Where it hands nothing, the
    // signature's type stands for what its arguments are
    const hs = [...dn.keys()], handed = new Set(hs.map(kindKey));
    // the value a method runs on too, where the call names it (`settings.mode.plan()` runs on a projection of `settings`)
    const recv = !site.on_receiver && arrowOf(step)?.receiver ? [on || arrowOf(step).receiver] : [];
    if (hs.length) for (const x of [...(arrowOf(step)?.takes || []), ...recv]) { const k = "t:" + x; if (m.dom.has(k) && !made.has(x) && !handed.has(kindKey(k))) m.dom.delete(k); }
    // what the step changes is the value it was handed, under the same name: `read_makefile(&mut g)`
    const changes = arrowOf(step)?.changes || [];
    for (const x of [...changes, ...(on && changes.includes(byId.get(step)?.cls) ? [on] : [])]) {
      const kind = kindKey(addPlace(typePlace(x))), held = [...dn.keys()].find(d => kindKey(d) === kind);
      for (const k of m.cod.keys()) if (held && kindKey(k) === kind && !cn.has(k)) cn.set(k, dn.get(held));
    }
  }
  // what the body's own code uses outside the calls that are its steps (`print(store.path)`, `if done:`, `Ok(path)`), as the
  // reader gives it: taken at the border by the names that hold them, inside the frame
  const uses = own && !leaf && !step.startsWith("op:") ? (byId.get(step)?.uses || []).filter(u => u.name) : [];
  for (const u of uses) if (u.type) addPlace({...typePlace(u.type), id: `t:${u.type}~${u.name}`, name: u.name});
  // what a site that raises gives is an error, made where the site makes it
  if (site?.raises) for (const g of site.gives || []) raised.add(addPlace(g.type ? (g.name ? {...typePlace(g.type), id: `t:${g.type}~${g.name}`, name: g.name} : typePlace(g.type)) : {id: "v:" + g.name, label: "·", kind: "type", hue: hueOf(null), type: null, name: g.name}));
  return {dom: m.dom, cod: m.cod, dn, cn, goes: m.goes, beyond, keep, kept, uses, errs, raised, inner: new Set([...inner].map(errKey))};
};
// build the nested picture: frames for open steps, pills for closed ones (siblings with the same places are one pill)
const pills = new Map(), frames = new Map();  // id -> {...}
const build = (node, parent, caller) => {
  if (node.synthetic) {
    const fid = "n:" + node.id, one = node.when.length === 1;
    const w = one ? node.when[0][1] : "", title = !one ? "one of" : !w ? "only sometimes" : w === "otherwise" ? "only otherwise" : `only when ${w}`;
    frames.set(fid, {id: fid, step: null, parent, hue: node.hue, choice: true, synthetic: true, title, whens: one ? [] : node.when.filter(w => w[1])});
    return node.kids.flatMap(k => build(k, fid, caller));
  }
  // in the finest plan a call without calls of its own is a pill; an open step has a frame and its own arrow
  if (node.open && !(fine && node.id !== V.route && !node.kids.length)) {
    const fid = "n:" + node.id, kids = [];
    // what the step shows closed, kept for the places it shows: opening it refines the code and leaves them where they stood
    const shut = fine || node.id === V.route || libraryClass(stepOf(node.id)) ? null : stepMorphism(node.id, false, false);
    frames.set(fid, {id: fid, step: node.id, parent, hue: node.hue, branch: node.branch, choice: node.choice, anchor: node.id === V.route, ...(node.parse ? {parse: true} : {}), closed: shut && {goes: shut.goes, cod: shut.cod, hb: !!parent && !caller && !/^(part|unit|op):/.test(node.id) && goesBack(node.id, shut.cod)}, title: (node.id === V.route ? entryTitle(node.id) : stepLabel(node.id)) + (node.choice ? " · one of" : "")});
    const lib = libraryClass(stepOf(node.id));
    // a library class that is the route starts at its public methods; met inside a route it is a call like another, with its own arrow
    const own = (lib && node.id === V.route) || node.id.startsWith("unit:") ? {dom: new Map(), cod: new Map(), goes: new Map()} : stepMorphism(node.id, true, node.id === V.route);
    const before = pills.size, back = node.kids.flatMap(k => build(k, fid, lib && node.id === V.route ? node.id : null));
    // opened, a step keeps the lines it had closed: its own arrow stands at its frame with what the closed box took and gave, so
    // the lines that crossed the box cross the frame; a value it hands on is at the moment the closed box had it, and a result
    // with no place of its own goes on to the step that called it from there
    if (caller) own.dom.set(addPlace(entryPlace(caller)), false);
    // in the finest plan the own arrow of an opened step is its border, not a step beside its steps: a value its
    // steps take from outside crosses the frame to them, and the own arrow keeps what its own body takes and what
    // the body gives back, handed on at the step's moment
    if (fine && node.id !== V.route && !lib) {
      const inD = new Set([...pills.values()].slice(before).flatMap(P => [...P.D.keys()]));
      for (const k of [...own.dom.keys()]) if (/^[tv]:/.test(k) && inD.has(k) && !own.keep?.has(k)) own.dom.delete(k);
    }
    // what the closed box took and handed on that no box inside it does crosses at its own pill; what one inside takes or
    // makes crosses the frame to it
    if (shut) {
      const inside = [...pills.values()].slice(before), inD = new Set(inside.flatMap(P => [...P.D.keys()])), inC = new Set(inside.flatMap(P => [...P.C.keys()]));
      for (const k of shut.dom.keys()) if (!own.dom.has(k) && !inC.has(k) && !inD.has(k)) own.dom.set(k, true);
      for (const k of shut.cod.keys()) if (!own.cod.has(k) && !inC.has(k)) own.cod.set(k, true);
    }
    const hb = fine ? !!parent && !caller && node.id !== V.route && !lib && !/^(part|unit|op):/.test(node.id) && goesBack(node.id, own.cod) : !!frames.get(fid).closed?.hb;
    const said = hb ? backName(node.id, own.cod) : null;
    if (own.dom.size || own.cod.size || hb || (back.length && !lib && !node.id.startsWith("unit:"))) {
      const oid = "o:" + node.id, own_ = {id: oid, steps: [node.id], D: own.dom, C: own.cod, dn: own.dn, cn: own.cn, goes: own.goes, parent: fid, hue: node.hue, own: true, beyond: own.beyond, keep: own.keep, ...(own.uses?.length ? {uses: own.uses} : {}), ...(own.errs?.size ? {errs: own.errs, inner: own.inner} : {}), ...(own.raised?.size ? {raised: own.raised} : {}), ...(said ? {said} : {}), ...(fine && node.id !== V.route && !lib ? {border: true} : {})};
      pills.set(oid, own_);
      for (const b of back) b.backTo = oid;
      if (hb) return [own_];
    }
    return [];
  }
  // a step beyond the cut stands for the image of its whole body, made as a closed step's is (`cutArrow`)
  const cut = fine && node.cut;
  const m = cut ? cutArrow(node.id) : stepMorphism(node.id, fine && !node.cut, false, fine && !node.cut), hb = parent && !caller && goesBack(node.id, m.cod);
  if (cut) for (const pl of m.places.values()) addPlace(pl);
  const said = hb ? backName(node.id, m.cod) : null;
  if (caller) m.dom.set(addPlace(entryPlace(caller)), false);
  // a unit stands even when nothing crosses its edge, so that it can be opened
  if (!m.dom.size && !m.cod.size && !hb && !node.id.startsWith("unit:")) return [];
  const pill = {id: "n:" + node.id, steps: [node.id], D: m.dom, C: m.cod, dn: m.dn, cn: m.cn, goes: m.goes || new Map(), parent, hue: node.hue, branch: node.branch, ...(said ? {said} : {}), ...(m.keep?.size ? {keep: m.keep} : {}), ...(m.errs?.size ? {errs: m.errs} : {}), ...(m.raised?.size ? {raised: m.raised} : {}), ...(m.kept?.size ? {kept: m.kept} : {}), ...(node.parse ? {parse: true} : {}), ...(cut ? {cut: true, mult: m.mult, split: m.split} : {})};
  pills.set(pill.id, pill);
  return hb ? [pill] : [];
};
for (const t of tree || routeTree()) build(t, null);
// Reading back is no exception (mathematics.md, rule 4): inside a step that reads back what it wrote, the read is of
// the place as the step's own writers left it, a value at a moment, made and taken inside the step. The place keeps
// what the step leaves there, in the codomain; closed, the value is internal to the step and draws no line
if (fine) {
  const backOf = new Map();
  for (const [fid, f] of frames) if (f.step && !f.synthetic && !f.anchor) { const rb = arrowOf(stepOf(f.step))?.reads_back; if (rb?.length) backOf.set(fid, new Set(rb)); }
  if (backOf.size) {
    const chainUp = fid => { const c = []; for (let f = fid; f; f = frames.get(f)?.parent) c.push(f); return c; };
    const writers = new Map();
    for (const P of pills.values()) for (const X of P.C.keys()) if (X.startsWith("w:")) { if (!writers.has(X)) writers.set(X, []); writers.get(X).push(P); }
    const versionOf = (X, fid) => {
      const id = `v:back:${X}~in~${fid}`;
      if (!places.has(id)) places.set(id, {id, label: `${places.get(X)?.label || X} · in ${frames.get(fid).title}`, kind: "type", hue: hueOf(null), type: null, at: fid + "/~back", stands: X});
      return id;
    };
    for (const P of pills.values()) for (const X of [...P.D.keys()].filter(k => k.startsWith("w:"))) {
      const ws = (writers.get(X) || []).filter(Q => Q !== P);
      const S = chainUp(P.parent).find(fid => backOf.get(fid)?.has(X) && ws.some(Q => chainUp(Q.parent).includes(fid)));
      if (!S) continue;
      const v = versionOf(X, S), r = P.D.get(X);
      P.D.delete(X); P.D.set(v, r);
      if (P.mult?.D.has(X)) { P.mult.D.set(v, P.mult.D.get(X)); P.mult.D.delete(X); }
      for (const Q of ws) if (chainUp(Q.parent).includes(S) && !Q.C.has(v)) { Q.C.set(v, Q.C.get(X)); if (Q.mult?.C.has(X)) Q.mult.C.set(v, Q.mult.C.get(X)); }
    }
  }
}
// a focus and the rest of the code: one arrow each way per unit in focus, counting the types that
// cross; an opened rest has one arrow per package in it, from the package that makes the types or
// to the one that takes them, and holds only the packages with an arrow
const restArrows = [];  // [rest pill, unit pill, side, types]
const around = V.focus.size ? aroundFocus() : [];
for (const [u, {in: ins, out: outs}] of restLinks) {
  for (const [r, ts] of [["~in", ins], ["~out", outs]]) {
    if (!ts.size) continue;
    if (!V.open.units.has(r)) { restArrows.push(["n:unit:" + r, "n:unit:" + u, r, ts]); continue; }
    const byPkg = new Map();
    for (const t of ts) for (const p of around) if ([...((r === "~in" ? usesOf().makes : usesOf().takes).get(t) || [])].some(x => inUnit(x, p))) { if (!byPkg.has(p)) byPkg.set(p, new Set()); byPkg.get(p).add(t); }
    for (const [p, s] of byPkg) restArrows.push([`n:unit:${r}:${p}`, "n:unit:" + u, r, s]);
  }
}
const linkedRest = new Set(restArrows.map(a => a[0]));
for (const P of [...pills.values()]) if (/^unit:~(in|out):/.test(P.steps[0]) && !linkedRest.has(P.id)) pills.delete(P.id);
// a type a step takes and hands on is two moments of the data, not one box: memory is versioned in
// call order (a step that hands on T makes a new T, later steps take the latest). The overview has no
// order between its routes and so no moments: a type is the joint itself, one box that what makes it
// and what takes it share, whatever is open. Zoom does not touch the moments: an opened family opens
// at each of them
const isMem = k => k.startsWith("v:") || k.startsWith("t:");
// a place as a step's own writers left it, read back inside that step: its moment is inside the step, fixed where it was made
const inside = k => k.startsWith("v:back:");
const version = (k, v) => { if (!places.has(v)) places.set(v, {...places.get(k), id: v}); return v; };
// the arms of a choice make one value, at the choice's moment, the value it hands on past it: a choice in a body is a frame
// of its own; a step that is itself a choice (a call into a family, which runs one of the overrides) makes it at a moment inside the step, and hands on a value
// of its own at the step's moment
const choiceMoment = fid => (frames.get(fid).synthetic ? fid : fid + "|");
// the arm of a choice in a body a step stands in, and the step's own moment: an opened step's arrow hands on at the moment
// the closed step did, so opening it moves no value to another moment
const armOf = P => { const F = P.own ? frames.get(P.parent) : P, c = frames.get(F?.parent); return c?.choice ? {choice: F.parent, branch: F.branch} : null; };
const ownMoment = P => (P.own ? P.parent : P.id);
// a value of a choice in a body is the choice's where it goes on past the choice and two of its arms make it, or where one
// arm makes anew a value the body named before, which the other arms keep (`if ..: series.push(..)`); a value one arm makes,
// or one each arm makes and uses itself, is its maker's
let armsMaking = null;
// the arms of a coproduct make one value however its variants differ: `Node::Bron(..)` in one arm, `Node::Begrip(..)` in the other
const valueOfArm = (c, k, names) => { const t = places.get(k)?.type, sum = t && SUM_OF.get(t); const nm = (names && names.get(k)) || ""; return `${c}|${sum ? "t:" + sum : kindKey(k)}|${nm.startsWith("#") ? "" : nm}`; };
const chainOf = P => planChain(P.parent);
const armsOf = () => {
  if (armsMaking) return armsMaking;
  armsMaking = new Map();
  const order = [...pills.values()], last = new Map();
  order.forEach((Q, i) => { const b = armOf(Q); if (b && frames.get(b.choice).synthetic) last.set(b.choice, i); });
  order.forEach(Q => {
    const b = armOf(Q);
    if (b && frames.get(b.choice).synthetic) for (const x of Q.C.keys()) if (isMem(x)) {
      const v = valueOfArm(b.choice, x, Q.cn);
      if (!armsMaking.has(v)) armsMaking.set(v, {arms: new Set(), on: false, last: new Map()});
      armsMaking.get(v).arms.add(b.branch);
      armsMaking.get(v).last.set(b.branch, Q);
    }
  });
  // what a step after the choice, in the body that holds it, takes of it
  order.forEach((Q, i) => {
    const ch = chainOf(Q);
    for (const [c, j] of last) if (i > j && !ch.includes(c) && ch.includes(frames.get(c).parent)) for (const x of Q.D.keys()) armsMaking.get(valueOfArm(c, x, Q.dn)) && (armsMaking.get(valueOfArm(c, x, Q.dn)).on = true);
  });
  return armsMaking;
};
const madeAt = (P, k, kept) => {
  const a = armOf(P);
  if (!a) return ownMoment(P);
  if (!frames.get(a.choice).synthetic) return choiceMoment(a.choice);
  // an error an arm hands on goes past the choice, whichever step of the arm fails: the errors of a choice are one value
  if (P.errs?.has(k)) return choiceMoment(a.choice);
  // of what one arm makes of a value, the last is what it hands on past the choice; what it makes first is made inside it
  const m = armsOf().get(valueOfArm(a.choice, k, P.cn));
  return kept || (m && m.on && m.arms.size > 1 && m.last.get(a.branch) === P) ? choiceMoment(a.choice) : ownMoment(P);
};
// in A, a value B makes by another step (`remade`) is one value: it stands at the moment of the step that makes it in B
const remadeAt = at => {
  const m = SIDE === "a" ? at.match(/^n:(.*)\/(\d+)$/) : null, to = m ? DIFF.remade[stepOf(m[1])]?.[m[2]] : null;
  return to == null ? at : `n:${m[1]}/${to}`;
};
// the values made so far, each in the scope of the step whose body made it: a name holds a value in that step and the steps
// inside it, and a step's own arrow hands on in the scope of its caller. A choice is no scope; its branches' values go on
const planChain = fid => { const c = []; for (let f = fid; f; f = frames.get(f)?.parent) c.unshift(f); return c; };
const latest = new Map(), madeIn = new Map(), armKey = new Map(), armValues = new Map();
const scopeOf = P => {
  // what a door of a class hands on goes back to the caller beyond the plan: the other doors, which have no order, do not take it
  if (isDoorOf(P.own ? frames.get(P.parent)?.step : P.steps[0])) return "door:" + P.id;
  let f = P.own ? frames.get(P.parent)?.parent : P.parent; while (f && frames.get(f)?.synthetic) f = frames.get(f).parent; return f || null;
};
// the arms of the choices in a body a step stands in: what one arm makes, no step of another arm of that choice takes
const armsAbove = P => { const out = new Map(); for (let F = P.own ? frames.get(P.parent) : P; F; F = frames.get(F.parent)) if (frames.get(F.parent)?.synthetic) out.set(F.parent, F.branch); return out; };
const latestSet = (key, v, P, scope = scopeOf(P), param = false) => { if (!latest.has(key)) latest.set(key, []); latest.get(key).push({v, scope, arms: armsAbove(P), param}); };
const latestGet = (key, P, inner = false, arms = null) => {
  const es = latest.get(key); if (!es) return null;
  const here = new Set(planChain(P.parent)), mine = arms || armsAbove(P);
  const apart = e => [...e.arms].some(([c, b]) => mine.has(c) && mine.get(c) !== b);
  // a parameter holds its value for the steps inside the body, not for the call that hands it, at the frame's own arrow
  const outside = e => !inner && e.param && P.own && P.parent === e.scope;
  for (let i = es.length - 1; i >= 0; i--) if ((!es[i].scope || here.has(es[i].scope)) && !apart(es[i]) && !outside(es[i])) return es[i].v;
  return null;
};
// what the call hands a step and what the function says it takes are one value when they are of one
// kind and at most one of them has a name: the value by the name and the type the call gives it.
// Two values of a kind with names of their own are two values
const oneOfKind = (m, names, keep) => {
  const out = new Map(), unnamed = new Map(), named = new Map();
  const has = k => !!names?.get(k);
  m.forEach((r, k) => {
    const kind = kindKey(k);
    // one name holds one value: what the arrow changes and what the call gives back under the name it was handed are one
    if (has(k)) { if ([...out.keys()].some(o => kindKey(o) === kind && names.get(o) === names.get(k))) return; if (!named.has(kind)) named.set(kind, k); out.set(k, r); return; }
    const prev = unnamed.get(kind);
    if (!prev) { unnamed.set(kind, k); out.set(k, r); return; }
    if (places.get(prev)?.type === kindOf(places.get(prev)?.type) && places.get(k)?.type !== places.get(prev)?.type) { out.delete(prev); unnamed.set(kind, k); out.set(k, r); }
  });
  for (const [kind, k] of unnamed) if (named.has(kind) && !keep?.has(k)) out.delete(k);
  return out;
};
for (const P of pills.values()) if (!P.steps[0].startsWith("unit:") && !P.cut) { P.D = oneOfKind(P.D, P.dn, P.keep); P.C = oneOfKind(P.C, P.cn); }
// planned by itself, a step does not know its caller's values: a value it takes by a name its body did not make is the
// caller's value of that name where the caller has one, else the latest of its kind the body made (`X ~y?`)
const condOf = (k, nm) => { const id = k + "?"; if (!places.has(id)) places.set(id, {...places.get(k), id, name: nm, cond: true}); return id; };
// a route's parameters are there from its start: a step handed one by its name takes it, unless the body named
// another value so first
// a door's parameters are there from the door's start, for the steps inside it
for (const P of pills.values()) for (const k of P.beyond || []) if (P.dn.get(k)) latestSet(kindKey(k) + "|" + P.dn.get(k), k, P, P.own ? P.parent : scopeOf(P));
// An override in a dispatch runs on the value its call runs on and takes what its call hands (mathematics.md, "Behavioural
// subtyping"): what its own signature takes of no name is the value the dispatch takes in its place. The receiver is the
// dispatch's of its kind; a parameter is the one the dispatch takes at its position, by the name the dispatch's own
// signature gives it, else the one of its kind
// the value a step takes for what it is handed by a name, else as the latest of its kind; false where it takes none
const bound = (P, k, nm = (P.dn && P.dn.get(k)) || "") => {
  const pl = places.get(k);
  // what a cut step takes by a name its body did not make: the value of that name here, else one the body made, inside it
  if (pl?.cond) { const v = latestGet(kindKey(k) + "|" + pl.name, P); return v || (alone ? null : false); }
  if (!isMem(k) || P.beyond?.has(k) || inside(k)) return null;
  // a name that holds a sum holds the sum, whichever variant the reader narrows it to
  const sum = nm && pl?.type && SUM_OF.get(pl.type), v = latestGet(kindKey(k) + "|" + nm, P) || (sum && latestGet("t:" + kindOf(sum) + "|" + nm, P));
  if (!v && P.kept?.has(k)) return false;
  const any = v ? null : latestGet(kindKey(k) + "|", P);
  return v || (any && alone && nm && !nm.startsWith("#") ? condOf(k, nm) : any);
};
const viaDispatch = (P, k) => {
  const key = P.own ? frames.get(P.parent)?.step : P.steps[0], d = key && treeInfo.get(key)?.dispatch, Own = d && pills.get("o:" + d);
  if (!Own || !isMem(k) || P.dn?.get(k) || P.keep?.has(k) || !places.get(k)?.type) return null;
  const takes = [...Own.D.keys()].filter(isMem).map(x => bound(Own, x) || x).filter(Boolean);
  const t = places.get(k).type, mine = byId.get(stepOf(key)), theirs = byId.get(stepOf(d));
  const args = n => (n?.params || []).filter(p => !p.receiver);
  const i = t === arrowOf(stepOf(key))?.receiver ? -1 : args(mine).findIndex(p => writesType(p, t));
  const by = i >= 0 && args(theirs)[i]?.name, named = by && takes.find(v => places.get(v)?.name === by);
  const recv = i < 0 && (treeInfo.get(d)?.on || arrowOf(stepOf(d))?.receiver), on = recv && takes.find(v => places.get(v)?.type === recv);
  return named || on || takes.find(v => kindKey(v) === kindKey(k)) || null;
};
// Inside an opened step a parameter holds the value its call hands it (#127): the call hands its values in order, the
// receiver first where the call names one, and each goes to the next parameter of its type; a value of a type no reader
// knows to the next parameter. Inside a method, its receiver is the latest of its kind until the body makes another
const entered = new Set();
const enter = fid => {
  if (!fid || entered.has(fid)) return;
  entered.add(fid);
  const f = frames.get(fid), info = f && !f.synthetic && !f.anchor && f.step && treeInfo.get(f.step), Own = info && pills.get("o:" + f.step);
  if (!info?.site || !Own || stepOf(f.step).startsWith("op:")) return;
  const fn = byId.get(stepOf(f.step)), recv = arrowOf(stepOf(f.step))?.receiver;
  const formals = [...(recv && !info.site.on_receiver && !info.built ? [{receiver: recv}] : []), ...(fn?.params || []).filter(p => !p.receiver)];
  // a value of a type no reader knows fills a parameter of no type of the code's
  const ours = p => !!p.receiver || (arrowOf(stepOf(f.step))?.takes || []).some(t => writesType(p, t));
  const fits = (p, h) => (!h.type ? !ours(p) : p.receiver ? p.receiver === h.type || kindOf(p.receiver) === kindOf(h.type) : writesType(p, h.type) || (SUM_OF.get(h.type) && writesType(p, SUM_OF.get(h.type))));
  let at = 0;
  const pairs = [], left = [];
  for (const h of info.site.hands || []) {
    const j = formals.findIndex((p, i) => i >= at && fits(p, h));
    if (j < 0) { left.push(h); continue; }
    at = j + 1;
    pairs.push([h, formals[j]]);
  }
  // what fits no parameter by its type fills one through a projection (`fileAll(ledger.rows)`): where as many are left
  // of each, they pair in order
  const open = formals.filter(p => !p.receiver && !pairs.some(([, q]) => q === p));
  if (left.length && left.length === open.length) left.forEach((h, i) => pairs.push([h, open[i]]));
  for (const [h, p] of pairs) {
    const k = h.type ? (h.name ? `t:${h.type}~${h.name}` : "t:" + h.type) : "v:" + h.name;
    if (!places.has(k)) continue;
    const v = bound(Own, k) || k;
    // a parameter is looked for by its own type and name: what fills it through a projection is of another type
    const x = !p.receiver && (arrowOf(stepOf(f.step))?.takes || []).find(t => writesType(p, t));
    if (p.receiver) latestSet(kindKey(v) + "|", v, Own, fid, true);
    else if (p.name) {
      latestSet((x ? "t:" + (x === V.route ? V.route : kindOf(x)) : v.startsWith("v:") ? "v:" + p.name : kindKey(v)) + "|" + p.name, v, Own, fid, true);
      // a parameter of no type of the code's is looked for as a value of no type, whatever it holds
      if (!x && !v.startsWith("v:")) latestSet("v:" + p.name + "|" + p.name, v, Own, fid, true);
    }
  }
};
for (const P of pills.values()) {
  // between units a type is the joint itself: one unit makes it, the next takes it
  if (P.steps[0].startsWith("unit:")) continue;
  for (const f of planChain(P.own ? frames.get(P.parent)?.parent : P.parent)) enter(f);
  // a value is taken by the name that holds it, else as the latest of its kind: an override runs on
  // the value its call runs on, and a subclass's value is its base's
  const Dv = new Map(), Cv = new Map(), name = (m, k) => (m && m.get(k)) || "", mult = P.mult && {D: new Map(), C: new Map()};
  P.D.forEach((r, k) => {
    const v = viaDispatch(P, k) || bound(P, k, name(P.dn, k));
    if (v === false) return;
    Dv.set(v || k, r);
    if (mult && P.mult.D.has(k)) mult.D.set(v || k, P.mult.D.get(k));
    // a value of a choice is what each arm made of it: a step after the choice takes them all
    if (armKey.has(v) && !armsAbove(P).has(places.get(v).at)) for (const w of armValues.get(armKey.get(v))) if (w !== v) Dv.set(w, r);
    if (v && P.keep?.has(k)) (P.kept ||= new Set()).add(v);
  });
  // a value the body uses itself is the one its name holds there, made inside or handed in; a name that holds none (a
  // result that went back, a literal) adds nothing
  // a use in an arm of a choice finds what that arm holds (`arms`, numbered as a site's)
  for (const u of P.uses || []) {
    const base = u.type ? `t:${u.type}~${u.name}` : "v:" + u.name, sum = u.type && SUM_OF.get(u.type);
    const arms = new Map([...armsAbove(P), ...(u.arms || []).map(([c, b]) => [`n:${frames.get(P.parent).step}~c${c}`, b])]);
    const v = latestGet(kindKey(base) + "|" + u.name, P, true, arms) || (sum && latestGet("t:" + kindOf(sum) + "|" + u.name, P, true, arms));
    if (!v) continue;
    if (!Dv.has(v)) Dv.set(v, false);
    (P.used ||= new Set()).add(v);
  }
  P.C.forEach((r, k) => {
    if (!isMem(k) || inside(k) || !V.route) { Cv.set(k, r); if (mult && P.mult.C.has(k)) mult.C.set(k, P.mult.C.get(k)); return; }
    const nm = name(P.cn, k), a = armOf(P);
    // a value of that name from before the choice, kept by the arms that do not make it anew; an opened step's arrow hands
    // on in its caller's scope
    const kept = !!a && !!nm && !nm.startsWith("#") && (() => { const was = latestGet(kindKey(k) + "|" + nm, P.own ? frames.get(P.parent) : P); return !!was && !madeIn.get(a.choice)?.has(was); })();
    const at = remadeAt(madeAt(P, k, kept)), v = version(k, `${k}@${at}`);
    if (a) { if (!madeIn.has(a.choice)) madeIn.set(a.choice, new Set()); madeIn.get(a.choice).add(v); }
    if (a && at === choiceMoment(a.choice)) { const ak = valueOfArm(a.choice, k, P.cn); armKey.set(v, ak); if (!armValues.has(ak)) armValues.set(ak, []); if (!armValues.get(ak).includes(v)) armValues.get(ak).push(v); }
    if (mult && P.mult.C.has(k)) mult.C.set(v, P.mult.C.get(k));
    Object.assign(places.get(v), {at}, P.errs?.has(k) || P.raised?.has(k) ? {err: true} : {});
    if (nm && !nm.startsWith("#") && !k.startsWith("v:")) Object.assign(places.get(v), {label: `${places.get(k).label} · ${nm}`, held: nm});
    latestSet(kindKey(k) + "|" + nm, v, P); latestSet(kindKey(k) + "|", v, P);
    Cv.set(v, r);
  });
  P.D = Dv; P.C = Cv;
  if (mult) P.mult = mult;
}
// Of what an opened body's steps make, its own arrow gives back what no step of the code inside takes: a value built only
// to be handed into another step (`Row(Reading(line))`) is that step's. One handed into a library's operation goes on in
// what the operation gives, and stays inside where that does (`query_map(.., |row| Ok(Expense { .. }))` gives the rows
// the body returns). Decided on the values at their moments, so a value made anew (`rates.insert(..)`) is not the one
// it was made from. A value from beyond the step that a step inside takes crosses the frame to that step, also where the
// signature takes it by no name (`report(job: Job, ..)` hands `job` on to `Message(..)`)
if (fine) {
  // an opened body takes every error its steps hand on that no step of it takes, each value by itself, up to the route
  for (const P of pills.values()) if (P.own && P.inner?.size) {
    const fid = P.parent, kinds = new Set([...P.inner].map(kindKey)), within = [...pills.values()].filter(Q => Q !== P && planChain(Q.parent).includes(fid));
    const madeHere = v => { const at = places.get(v)?.at; return !!at && (at.startsWith(fid + "/") || at.startsWith(fid + "|") || at.startsWith(fid + "~c")); };
    for (const Q of within) for (const v of Q.C.keys()) if (places.get(v)?.err && madeHere(v) && kinds.has(kindKey(v)) && !within.some(R => R.D.has(v))) { P.D.set(v, false); (P.used ||= new Set()).add(v); }
  }
  const isOp = Q => stepOf(Q.steps[0]).startsWith("op:");
  // the route's own arrow is a point on its border too: what it gives back is what no step inside takes
  for (const P of pills.values()) if (P.own && (P.border || frames.get(P.parent)?.anchor)) {
    const fid = P.parent, within = [...pills.values()].filter(Q => Q !== P && planChain(Q.parent).includes(fid));
    const stays = (v, seen = new Set()) => !seen.has(v) && (seen.add(v), within.some(Q => Q.D.has(v) && (!isOp(Q) || (Q.C.size > 0 && [...Q.C.keys()].every(c => stays(c, seen))))));
    const madeHere = v => { const at = places.get(v)?.at; return !!at && (at.startsWith(fid + "/") || at.startsWith(fid + "|") || at.startsWith(fid + "~c")); };
    for (const v of P.kept || []) if (madeHere(v) && stays(v) && !P.used?.has(v)) P.D.delete(v);
    // what the signature takes by no name from beyond is the parameter a step inside takes by its name
    const beyond = v => !places.get(v)?.at && !places.get(v)?.name && v.startsWith("t:");
    const crosses = v => within.some(Q => Q.D.has(v) || (beyond(v) && [...Q.D.keys()].some(w => !places.get(w)?.at && places.get(w)?.type === places.get(v).type)));
    if (P.border) for (const v of [...P.D.keys()]) if (isMem(v) && !madeHere(v) && crosses(v) && !P.used?.has(v)) P.D.delete(v);
  }
}
// zoom does not touch the moments: they are counted on the values themselves, and then a value
// stands where the view puts its type, a closed kind at each of its moments
const standsAt = k => {
  const p = places.get(k);
  if (!p?.type || !k.startsWith("t:")) return k;
  const pid = placeOfType(p.type), id = "t:" + pid + momentOf(k);
  if (id === k) return k;
  if (!places.has(id)) { const q = typePlace(pid); places.set(id, {...q, id, label: q.label + (p.held ? ` · ${p.held}` : "")}); }
  return id;
};
const toShown = m => { const out = new Map(); m.forEach((r, k) => { const k2 = standsAt(k); out.set(k2, out.has(k2) ? out.get(k2) && r : r); }); return out; };
if (!fine) for (const P of pills.values()) { P.D = toShown(P.D); P.C = toShown(P.C); }
// a value no step makes is its type, from beyond; it keeps the name that holds it where a step takes two of one kind: they
// are two values (`consume(a, b)` takes two records, `report(c, q)` a Circle and a Square), also once their family closes
// them into one box
const split = new Set();
if (fine) {
  const unmade = k => k.startsWith("t:") && places.get(k)?.name && !places.get(k).at;
  for (const P of pills.values()) for (const m of [P.D, P.C]) {
    // a step beyond the cut takes what the steps of its body take, each by itself: they decide, in its plan
    if (P.cut) { for (const t of P.split) split.add(t); continue; }
    const seen = new Map();
    for (const k of m.keys()) if (unmade(k)) { const t = kindOf(places.get(k).type); if (!seen.has(t)) seen.set(t, new Set()); seen.get(t).add(places.get(k).name); }
    for (const [t, ns] of seen) if (ns.size > 1) split.add(t);
  }
  return {places, pills, frames, found, updated, split};
}
// what the steps do at each place of the world: read it, write it, go there without saying which
// way; whether a route finds what it reads there; whether a step that reads it hands on what it read
const roles = new Map();
const role = k => { if (!roles.has(k)) roles.set(k, {reads: false, writes: false, open: false, used: false, found: found.has(k), updated: updated.has(k)}); return roles.get(k); };
// a place the outermost opened step shows closed has the role it had there: what is done to it inside is that step's
// doing, and opening the step does not make the place another's
const countRole = (k, s, handsOn) => {
  const r = role(k);
  r.reads ||= s.has("read"); r.writes ||= s.has("write"); r.open ||= s.has("open");
  if (s.has("use")) r.used ||= handsOn;
};
const shutOver = P => { let top = null; for (let f = P.parent; f; f = frames.get(f)?.parent) if (frames.get(f)?.closed) top = frames.get(f).closed; return top; };
const counted = new Set();
for (const P of pills.values()) {
  const top = shutOver(P);
  if (top && !counted.has(top)) { counted.add(top); for (const [k, s] of top.goes) countRole(k, s, top.hb || [...top.cod.keys()].some(c => c !== k)); }
  for (const [k, s] of P.goes || []) if (!top?.goes.has(k)) countRole(k, s, !!P.backTo || [...P.C.keys()].some(c => c !== k));
}
return {places, pills, frames, restArrows, roles};
};
// ---- a route is one finest plan, and every picture of it is its image under the quotient the reader has chosen ----
// Occurrences (the steps of the route, each a call), values (a type, the name that holds it, the moment it was made) and the
// places of the world are those of the finest plan `F`; what is open decides `q`, and a closed step is the block of the
// occurrences below it. Its domain is what the occurrences below it take and none of them makes, its codomain what its own
// arrow hands on and what they write: the lines that cross the block's border. A value made and taken inside a block is
// internal to it. Where a box stands, which side a place of the world is on and when a value was made are read off `F`.
const isOpenKey = key => key === V.route || V.open.steps.has(key) || V.open.functions.has(stepOf(key));
// `fineKeys`: the image of one step planned by itself (`cutArrow`), its places the finest plan's, not the view's
const imageOf = (F, opened, folded, {fineKeys = false} = {}) => {
  const {places: fp, pills: fPills, frames: fFrames} = F;
  const closedAt = new Map();
  // the outermost closed step above a frame, or the frame itself when it is closed and nothing above it is
  const closedOf = fid => {
    if (!fid) return null;
    if (closedAt.has(fid)) return closedAt.get(fid);
    const f = fFrames.get(fid), up = closedOf(f.parent), r = up || (f.synthetic || opened(f.step) ? null : fid);
    closedAt.set(fid, r);
    return r;
  };
  // the steps that only parse the command line are the entry's, while it is closed: the values they make are the entry's
  const entryId = "e:" + V.route;
  const hidden = new Set();
  if (folded) {
    const mark = fid => { for (const f of fFrames.values()) if (f.parent === fid && !hidden.has(f.id)) { hidden.add(f.id); mark(f.id); } };
    for (const f of fFrames.values()) if (f.parse) { hidden.add(f.id); mark(f.id); }
    for (const P of fPills.values()) if (P.parse || hidden.has(P.parent)) hidden.add(P.id);
  }
  const qId = new Map();
  // a value keeps its name where a step made it, and where two of its type are taken at once
  const named = pl => !!pl.name && !pl.name.startsWith("#") && (!!pl.at || F.split.has(kindOf(pl.type))), nameOf_ = pl => named(pl) ? "~" + pl.name : "";
  const q = k => {
    if (fineKeys) return k;
    if (qId.has(k)) return qId.get(k);
    const pl = fp.get(k);
    let id = k;
    if (pl && /^[tv]:/.test(k)) id = pl.at && hidden.has(pl.at) ? entryId : pl.type && k.startsWith("t:") ? "t:" + placeOfType(pl.type) + nameOf_(pl) + (pl.door ? "^" + pl.door : "") + (pl.at ? "@" + pl.at : "") : k;
    qId.set(k, id);
    return id;
  };
  const places = new Map();
  const placeOf = (k, id) => {
    if (places.has(id)) return id;
    if (fineKeys) { places.set(id, fp.get(k)); return id; }
    const pl = fp.get(k);
    if (id === entryId) places.set(id, entryPlace(V.route));
    else if (pl?.type && k.startsWith("t:")) {
      const pid = placeOfType(pl.type), b = typePlace(pid);
      places.set(id, {...b, id, label: b.label + (pl.held ? ` · ${pl.held}` : named(pl) ? ` · ${pl.name}` : ""), at: pl.at, name: named(pl) ? pl.name : undefined, held: pl.held});
    } else places.set(id, fp.get(k));
    return id;
  };
  // the own arrow of an opened step uses a value where the value was made: inside the step what its steps made there,
  // outside it what it hands on at the step's moment or takes from beyond
  const userFrame = (P, k) => {
    const F0 = fFrames.get(P.parent);
    if (!P.own || F0?.anchor) return P.parent;
    const at = fp.get(k)?.at;
    return P.border && at && (at.startsWith(P.parent + "/") || at.startsWith(P.parent + "|") || at.startsWith(P.parent + "~c")) ? P.parent : F0?.parent;
  };
  const users = new Map(), makers = new Map();
  const note = (id, P, maker, k) => { const m = maker ? makers : users; if (!m.has(id)) m.set(id, []); m.get(id).push(userFrame(P, k)); if (maker) note(id, P, false, k); };
  const famId = pl => { const f = pl?.type && frameOf(pl.type); return f ? "t:" + f + nameOf_(pl) + (pl.door ? "^" + pl.door : "") + (pl.at ? "@" + pl.at : "") : null; };
  const within = (at, S) => !!at && (at.startsWith(S + "/") || at.startsWith(S + "|") || at.startsWith(S + "~c"));
  const pills = new Map(), frames = new Map();
  for (const [fid, f] of fFrames) if (!closedOf(fid) && !hidden.has(fid)) frames.set(fid, {...f, closed: undefined});
  const add = (m, k, r) => { const id = q(k); placeOf(k, id); m.set(id, m.has(id) ? m.get(id) && r : r); return id; };
  // the lines a block draws on the finest plan, by channel: what does not change with what else is open
  const fineOf = incidence => incidence.map(([way, , k, r]) => `${way}${r ? "~" : ""}${k}`).sort().join(" ");
  const blocks = new Map(), pillBlock = new Map(), inc = new Map(), drop = new Map();
  for (const P of fPills.values()) {
    if (hidden.has(P.id)) continue;
    const c = closedOf(P.parent), id = c || P.id;
    if (!blocks.has(id)) blocks.set(id, {id, frame: c ? fFrames.get(c) : null, members: []});
    blocks.get(id).members.push(P);
    pillBlock.set(P.id, id);
  }
  for (const B of blocks.values()) {
    const D = new Map(), C = new Map(), Dn = new Map(), Cn = new Map(), Dr = new Map(), Cr = new Map(), goes = new Map(), S = B.frame, back = S ? new Set(arrowOf(stepOf(S.step))?.reads_back || []) : null;
    const incidence = [];
    const hands = S ? treeInfo.get(S.step)?.site?.hands || [] : [];
    const handed = pl => !!pl && hands.some(h => (h.name && !h.name.startsWith("#") ? h.name === pl.name : !!h.type && h.type === pl.type));
    for (const P of B.members) {
      const mine = !S || P.id === "o:" + S.step;
      for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const [k, r] of m) {
        const pl = fp.get(k), isValue = /^[tv]:/.test(k);
        // a value made and taken inside the block is the block's own; a place it wrote and reads back is no input of it
        if (S && isValue && within(pl?.at, S.id) && !hidden.has(pl.at)) { drop.set(`${P.id}|${way}|${k}`, "internal"); continue; }
        if (S && way === "D" && !isValue && back.has(k)) { drop.set(`${P.id}|${way}|${k}`, "back"); continue; }
        // a value the step's own call is handed is its own arrow's input, whichever of its steps takes it inside
        incidence.push([way, P, k, mine ? r : !(way === "D" && isValue && handed(pl)), mine]);
      }
      // what the block reads back of what it wrote is no read of the place: the place stays in the block's codomain
      if (back) P.goes?.forEach((s, k) => { if (!goes.has(k)) goes.set(k, new Set()); for (const x of s) if (!back.has(k) || (x !== "read" && x !== "use")) goes.get(k).add(x); });
      else joinGoes(goes, P.goes);
    }
    for (const [way, P, k, r, mine] of incidence) {
      const id = add(way === "D" ? D : C, k, r);
      // the fine lines that one line of the picture stands for, and of them the ones it reaches through a step it calls; a
      // step beyond the cut brings the lines its body draws there, by channel
      const n = way === "D" ? Dn : Cn, far = way === "D" ? Dr : Cr, [c, cr] = P.mult?.[way]?.get(k) || [1, r ? 1 : 0];
      n.set(id, (n.get(id) || 0) + c);
      const reached = mine ? cr : r ? c : 0;
      if (reached) far.set(id, (far.get(id) || 0) + reached);
      inc.set(`${P.id}|${way}|${k}`, {way, P: P.id, k, p: B.id, c: id});
      note(id, P, way === "C", k);
      const f = famId(fp.get(k));
      if (f) note(f, P, way === "C", k);
    }
    const out = B.members.find(P => P.backTo && !B.members.some(Q => Q.id === P.backTo));
    const back_ = out?.backTo;
    if (!S) {
      const P = B.members[0];
      if (!D.size && !C.size && !back_ && !P.own) continue;
      pills.set(P.id, {...P, D, C, Dn, Cn, Dr, Cr, fine: fineOf(incidence), goes, ...(back_ ? {backTo: back_, backFrom: out.id, said: out.said} : {backTo: undefined})});
      continue;
    }
    if (!D.size && !C.size && !back_) continue;
    pills.set(S.id, {id: S.id, steps: [S.step], D, C, Dn, Cn, Dr, Cr, fine: fineOf(incidence), dn: new Map(), cn: new Map(), goes, parent: S.parent, hue: S.hue, ...(back_ ? {backTo: back_, backFrom: out.id, said: out.said} : {})});
  }
  for (const P of [...pills.values()]) if (P.backTo && !pills.has(P.backTo)) delete P.backTo;
  for (const P of pills.values()) if (P.backTo) inc.set(`${P.backFrom}|back`, {way: "B", P: P.backFrom, k: P.backTo, p: P.id, c: P.backTo});
  // places with the same incidence are one block of the quotient with a multiplicity: the values that only one closed step takes
  // and no step makes are the same block (one neighbour, no maker), drawn as a count on the step and drawn apart when it opens
  const joined = new Map(), at = new Map();
  for (const i of inc.values()) {
    if (i.way === "B") continue;
    const e = at.get(i.c) || {ways: new Set(), blocks: new Set()};
    e.ways.add(i.way); e.blocks.add(i.p); at.set(i.c, e);
  }
  const free = new Map();
  for (const [id, e] of at) {
    const pl = places.get(id);
    if (e.ways.size !== 1 || !e.ways.has("D") || e.blocks.size !== 1 || !pl || !/^[tv]:/.test(id) || (pl.type && (opens(pl.type) || pl.type === V.route)) || famId(pl)) continue;
    const b = [...e.blocks][0], P = pills.get(b);
    if (!P || P.own || /^(unit|part):/.test(P.steps[0])) continue;
    if (!free.has(b)) free.set(b, []);
    free.get(b).push(id);
  }
  for (const [b, ids] of free) {
    if (ids.length < 2 || fineKeys) continue;
    const P = pills.get(b);
    for (const id of ids) { P.D.delete(id); P.Dn?.delete(id); P.Dr?.delete(id); places.delete(id); joined.set(id, b); }
    P.folded = ids.length;
  }
  // where a box stands: in the smallest frame that holds all the steps that use it and the one that made it; a value no step
  // inside a step makes stands outside it, and a place of the world outside every step
  const chainOf = fid => { const c = []; for (let f = fid; f; f = fFrames.get(f)?.parent) c.unshift(f); return c; };
  const stepFrame = f => !fFrames.get(f).synthetic && !fFrames.get(f).anchor;
  const homeOf = (ids, world) => {
    const cs = ids.flatMap(i => users.get(i) || []).map(chainOf), mk = ids.flatMap(i => makers.get(i) || []).map(chainOf);
    if (!cs.length || cs.some(c => !c.length)) return null;
    let common = cs[0];
    for (const c of cs.slice(1)) { let i = 0; while (i < common.length && i < c.length && common[i] === c[i]) i++; common = common.slice(0, i); }
    const i = common.findIndex(f => stepFrame(f) && (world || !mk.some(c => c.includes(f))));
    if (i >= 0) common = common.slice(0, i);
    while (common.length && !frames.has(common[common.length - 1])) common.pop();
    return common.length ? common[common.length - 1] : null;
  };
  return {places, pills, frames, homeOf, hidden, blocks, pillBlock, inc, drop, q, folded: joined};
};
// A step beyond the cut is the image of its whole body, made by the same `planBase` and `imageOf` as every closed step: its
// body unfolded one level, each of its calls cut in turn. Closing is a composite of closings (closing the calls, then the
// step, is closing the step), so this is the image of the body unfolded to its leaves, made without unfolding it: once for
// each function, call and set of callers that make its paths whole, whichever occurrence asks
const cutArrows = new Map(), makerSets = new Map(), siteIds = new WeakMap();
// the callers whose calls make a path whole for a touch the step reaches: the only ones its arrow depends on
const pathMakers = (step, on, alts = null) => {
  const k = `${SIDE}|${step}@${on || ""}${alts ? "!" + alts : ""}`;
  if (!makerSets.has(k)) makerSets.set(k, new Set([...reach(step, on, null, null, alts).nodes].flatMap(id => (byId.get(id)?.touches || []).flatMap(t => (t.by || []).flatMap(f => f.split("&"))))));
  return makerSets.get(k);
};
let sites = 0;
const siteId = site => { if (!site) return "-"; if (!siteIds.has(site)) siteIds.set(site, ++sites); return siteIds.get(site); };
const cutArrow = key => {
  const i = treeInfo.get(key), made = pathMakers(i.step, i.on, callAlts(key));
  const memo = [SIDE, i.step, i.on || "", siteId(i.site), [...(i.fed || [])].sort(), [...i.path, i.step].filter(f => made.has(f)).sort()].join("|");
  if (!cutArrows.has(memo)) {
    const F = planBase(true, [unfoldStep(key)]), P = imageOf(F, () => false, false, {fineKeys: true}).pills.get("n:" + key);
    // what the step hands on is at its own moment; the moment it has is its caller's to give
    const at = "@n:" + key, strip = k => (k.endsWith(at) ? k.slice(0, -at.length) : k);
    const r = {key, dom: new Map(), cod: new Map(), dn: new Map(), cn: new Map(), goes: P?.goes || new Map(), mult: {D: new Map(), C: new Map()}, places: new Map(), split: F.split};
    for (const [way, m, n, far, names, into] of [["D", P?.D, P?.Dn, P?.Dr, r.dn, r.dom], ["C", P?.C, P?.Cn, P?.Cr, r.cn, r.cod]]) for (const [k, flag] of m || []) {
      const s = strip(k), pl = F.places.get(k), nm = way === "D" ? pl?.name : pl?.held;
      into.set(s, flag);
      r.mult[way].set(s, [n?.get(k) || 1, far?.get(k) || 0]);
      r.places.set(s, F.places.get(s) || {...pl, id: s, at: undefined});
      if (nm) names.set(s, nm);
    }
    cutArrows.set(memo, r);
  }
  // each call that makes a figure makes one of its own: the figures of another call of the step are this call's
  const r = cutArrows.get(memo), fig = "w:Screen:fig:", re = k => (r.key === key ? k : k.replace(fig + r.key, fig + key));
  const copy = m => new Map([...m].map(([k, v]) => [re(k), v]));
  return {dom: copy(r.dom), cod: copy(r.cod), dn: copy(r.dn), cn: copy(r.cn), goes: copy(r.goes), mult: {D: copy(r.mult.D), C: copy(r.mult.C)}, places: new Map([...r.places].map(([k, pl]) => [re(k), {...pl, id: re(k)}])), split: r.split};
};
// Opening one more box must refine the picture and nothing else. For each box that can be opened (a closed step, a closed
// family), the picture with that box open is taken from the same finest plan and held to the one that is drawn: each of its
// boxes lies in one box of the drawn one, every line of the drawn one is the image of a line of the open one, and no line of
// the open one that crosses a block's border is missing from the drawn one
const refines = (A, B) => {
  const out = [], phi = [new Map(), new Map()];
  const lines = M => new Set([...M.inc.values()].filter(i => !M.folded.has(i.c)).map(i => (i.way === "D" ? `${i.c}>${i.p}` : `${i.p}>${i.c}`)));
  const mapTo = (i, way) => { const a = A.pillBlock.get(i.P), c = i.way === "B" ? A.pillBlock.get(i.k) : (A.folded.get(A.q(i.k)) || A.q(i.k)); return a && c ? (way === "D" ? `${c}>${a}` : `${a}>${c}`) : null; };
  for (const i of B.inc.values()) {
    const a = A.pillBlock.get(i.P), c = i.way === "B" ? A.pillBlock.get(i.k) : A.q(i.k);
    for (const [n, from, to] of [[0, i.p, a], [1, i.c, c]]) if (to) { if (phi[n].has(from) && phi[n].get(from) !== to) out.push(`box ${from} lies in ${phi[n].get(from)} and in ${to}`); phi[n].set(from, to); }
  }
  const la = lines(A), img = new Set();
  for (const [key, i] of B.inc) {
    // what a block made and takes inside is no line of its border
    const gone = A.drop.get(key);
    if (gone === "internal" && !A.places.has(A.q(i.k))) continue;
    const l = mapTo(i, i.way === "D" ? "D" : "C");
    if (l) img.add(l);
  }
  for (const l of la) if (!img.has(l)) out.push(`lost ${l}`);
  for (const l of img) if (!la.has(l)) { const [x, y] = l.split(">"); if (x !== y) out.push(`more ${l}`); }
  return out;
};
const checkOpenings = (F, view, folded) => {
  const faults = [], open = V.open, steps = new Set(open.steps), fams = new Set(open.families);
  const tries = [
    ...[...view.blocks.values()].filter(B => B.frame && !B.frame.synthetic).map(B => ["step", B.frame.step, () => open.steps.add(B.frame.step)]),
    ...[...new Set([...view.places.values()].filter(p => p.type && opens(p.type) && !open.families.has(p.type)).map(p => familyOf(p.type)))].map(f => ["family", f, () => open.families.add(f)]),
  ];
  for (const [kind, what, apply] of tries) {
    try { apply(); const B = imageOf(F, isOpenKey, folded); for (const f of refines(view, B)) faults.push(`opening ${kind} ${what}: ${f}`); }
    finally { open.steps = new Set(steps); open.families = new Set(fams); }
  }
  return faults;
};
const coarsen = F => {
  const foldable = ["cli", "script"].includes(byId.get(V.route)?.entry?.kind), folded = foldable && !V.open.places.has("e:" + V.route);
  const view = imageOf(F, isOpenKey, folded);
  // which side a place of the world stands on is decided by the coarsest picture, where only the route is open
  const top = imageOf(F, key => key === V.route, foldable);
  const roles = new Map();
  const role = k => { if (!roles.has(k)) roles.set(k, {reads: false, writes: false, open: false, used: false, found: F.found.has(k), updated: F.updated.has(k)}); return roles.get(k); };
  for (const P of top.pills.values()) for (const [k, s] of P.goes || []) {
    // whether a read is used is a fact of the step's body: what it gives reaches the step's codomain, or it is a check
    const r = role(k);
    r.reads ||= s.has("read"); r.writes ||= s.has("write"); r.open ||= s.has("open"); r.used ||= s.has("use");
  }
  // the storage quotient and the fold of the disk are taken on the places of the finest plan, whatever is open
  const ways = new Map();
  for (const P of F.pills.values()) for (const [way, m] of [["in", P.D], ["out", P.C]]) for (const k of m.keys()) { if (!ways.has(k)) ways.set(k, new Set()); ways.get(k).add(way); }
  return {places: view.places, pills: view.pills, frames: view.frames, restArrows: [], folded: view.folded, roles, home: view.homeOf, fine: {places: F.places, ways}, oracle: window.ATLAS_ORACLE ? checkOpenings(F, view, folded) : null};
};
const plan = () => {
  if (!V.route) return planBase(false);
  FINE = true;
  FIBER = null;
  cutArrows.clear();
  try {
    const F = planBase(true);
    FIBER = new Set([...F.places.values()].map(p => p.type).filter(Boolean));
    const C = coarsen(F);
    window.atlasPlan = {fine: F, coarse: C};
    return {...C, fiber: FIBER};
  } finally { FINE = false; }
};

// where a place of the world stands, by what the route does with its content (counted on the coarsest picture): left what
// it finds and uses, right what it leaves there, between the steps what one leaves for a later one to use, twice what it
// finds and changes; one the code goes to without saying which way stands between
const sideOfRoles = (rs, otherwise) => {
  const any = f => rs.some(r => r[f]);
  // what it only makes sure of before it reads it (a file downloaded where it is missing) is its input, between the steps
  // that fill and use it
  // what it reads there only to check or count it does not use: the place keeps what the route leaves there
  if (any("reads") && any("writes")) return !any("used") ? "out" : any("found") && any("updated") ? "both" : "mid";
  if (any("reads")) return "in";
  if (any("writes")) return "out";
  if (any("open")) return "mid";
  return otherwise();
};
