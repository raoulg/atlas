//! The laws of what a route does, each computed along two routes that must agree
//! (`docs/reference/mathematics.md`). The subjects are graphs made here, never a program: a handful of
//! functions whose bodies touch two files, call one another, choose and loop, drawn by `proptest`
//! (`PROPTEST_CASES` sets how many, 256 by default) and shrunk to the smallest that breaks a law.
//!
//! - **The effect functor** keeps composition: what a function does at a place, worked out from what
//!   its calls do there (`flow::arrows`), is what the one body with every call written out in its
//!   place does there. Any bracketing of the same steps is so one arrow.
//! - **Reach** is the transitive closure of the calls: the least relation that holds every call and
//!   is transitive.
//! - **Gluing** is a pushout: what lies beyond the border of a library is glued into the function at
//!   the border, so what a program's function reaches at places is the same however deep the library
//!   is read.

use std::collections::{BTreeMap, BTreeSet};

use atlas_engine::diff::Status;
use atlas_engine::flow::arrows;
use atlas_engine::view::{reach_from, reached, View};
use atlas_engine::{Dir, Edge, EdgeKind, Graph, Node, NodeKind, Site, Touch, World};
use proptest::prelude::*;

const PLACES: [&str; 2] = ["data/a.txt", "data/b.txt"];

fn touch(dir: Dir, place: usize, line: u32) -> Touch {
    Touch { artifact: PLACES[place].into(), line, ..Touch::new(World::Disk, Some(dir)) }
}

/// A step of a body as the test writes it: a touch of one of `PLACES`, a call of a function further
/// down the list (so the calls have no cycle), a choice of arms, or a loop.
#[derive(Clone, Debug)]
enum Item {
    Touch(bool, usize),
    Call(usize),
    Choice(Vec<Vec<Item>>),
    Loop(Vec<Item>),
}

/// The items of a body of function `at` of `n`: its calls reach only the functions after it.
fn items(at: usize, n: usize) -> impl Strategy<Value = Vec<Item>> {
    let leaf = if at + 1 < n {
        prop_oneof![
            3 => (any::<bool>(), 0..PLACES.len()).prop_map(|(w, p)| Item::Touch(w, p)),
            2 => (at + 1..n).prop_map(Item::Call),
        ]
        .boxed()
    } else {
        (any::<bool>(), 0..PLACES.len()).prop_map(|(w, p)| Item::Touch(w, p)).boxed()
    };
    let item = leaf.prop_recursive(2, 12, 3, |inner| {
        prop_oneof![
            proptest::collection::vec(proptest::collection::vec(inner.clone(), 0..3), 1..3).prop_map(Item::Choice),
            proptest::collection::vec(inner, 1..3).prop_map(Item::Loop),
        ]
    });
    proptest::collection::vec(item, 0..5)
}

/// Up to four functions `f0`..`f3`, each a body of items; `f0` is the route.
fn programs() -> impl Strategy<Value = Vec<Vec<Item>>> {
    (1usize..5).prop_flat_map(|n| (0..n).map(|i| items(i, n)).collect::<Vec<_>>())
}

/// A body laid out on lines, as a reader lays out a function: its touches and calls, each on a line
/// of its own, and the lines its arms and loops span. An arm and a loop open on a line of their own,
/// so that one with no steps still spans a line.
#[derive(Default)]
struct Laid {
    line: u32,
    choices: u32,
    node: Node,
}

impl Laid {
    /// Lays out `items`; a call is written out in its place (`inline`) or is a call of its function.
    fn lay(&mut self, items: &[Item], arms: &[(u32, u32)], program: &[Vec<Item>], inline: bool) {
        for it in items {
            match it {
                Item::Touch(write, p) => {
                    self.node.touches.push(touch(if *write { Dir::Write } else { Dir::Read }, *p, self.line));
                    self.line += 1;
                }
                Item::Call(f) if inline => self.lay(&program[*f], arms, program, true),
                Item::Call(f) => {
                    self.node.sites.push(Site { to: format!("f{f}"), line: self.line, arms: arms.to_vec(), ..Default::default() });
                    self.line += 1;
                }
                Item::Choice(alts) => {
                    self.choices += 1;
                    let c = self.choices;
                    for (k, arm) in alts.iter().enumerate() {
                        let start = self.line;
                        self.line += 1;
                        let inner: Vec<(u32, u32)> = arms.iter().copied().chain([(c, k as u32)]).collect();
                        self.lay(arm, &inner, program, inline);
                        self.node.arm_lines.push((start, self.line - 1));
                        self.node.arm_choices.push(c);
                    }
                }
                Item::Loop(body) => {
                    let start = self.line;
                    self.line += 1;
                    self.lay(body, arms, program, inline);
                    self.node.loop_lines.push((start, self.line - 1));
                }
            }
        }
    }
}

fn function(id: &str) -> Node {
    Node { id: id.into(), kind: NodeKind::Function, name: id.into(), module: "m".into(), ..Default::default() }
}

fn laid(id: &str, items: &[Item], program: &[Vec<Item>], inline: bool) -> Node {
    let mut l = Laid { line: 1, node: function(id), ..Default::default() };
    l.lay(items, &[], program, inline);
    l.node
}

fn calls(src: &str, dst: &str) -> Edge {
    Edge { src: src.into(), dst: dst.into(), kind: EdgeKind::Calls, via: None, weight: None, arms: vec![], uses: false }
}

/// The program as a graph of its functions, each calling the others.
fn composed(program: &[Vec<Item>]) -> Graph {
    let nodes: Vec<Node> = program.iter().enumerate().map(|(i, b)| laid(&format!("f{i}"), b, program, false)).collect();
    let edges = nodes.iter().flat_map(|n| n.sites.iter().map(|s| calls(&n.id, &s.to))).collect();
    Graph { nodes, edges, ..Default::default() }
}

/// The route as one function, every call written out in its place.
fn inlined(program: &[Vec<Item>]) -> Graph {
    Graph { nodes: vec![laid("f0", &program[0], program, true)], ..Default::default() }
}

/// What a function does at places as its arrow says: where it reads only what it wrote, and where it
/// writes on what it found.
fn at_places(g: &Graph, id: &str) -> (Vec<String>, Vec<String>) {
    arrows(g).get(id).map(|a| (a.reads_back.clone(), a.updates.clone())).unwrap_or_default()
}

proptest! {
    #[test]
    fn the_effect_functor_keeps_composition(program in programs()) {
        prop_assert_eq!(at_places(&composed(&program), "f0"), at_places(&inlined(&program), "f0"), "{:#?}", program);
    }
}

// ---------- Yoneda: what changed ----------

const CLASSES: usize = 3;
const FUNCTIONS: usize = 3;

/// The arrows a reading may hold between three classes `m.C*` and three functions `m.f*`: what a
/// function takes, gives, changes and builds, what a class holds and extends, and the calls.
fn candidates() -> Vec<(String, String, EdgeKind)> {
    let (c, f) = (|i: usize| format!("m.C{i}"), |i: usize| format!("m.f{i}"));
    let mut out = vec![];
    for i in 0..FUNCTIONS {
        for j in 0..CLASSES {
            for kind in [EdgeKind::Takes, EdgeKind::Returns, EdgeKind::Mutates, EdgeKind::Instantiates] {
                out.push((f(i), c(j), kind));
            }
        }
        for j in (0..FUNCTIONS).filter(|j| *j != i) {
            out.push((f(i), f(j), EdgeKind::Calls));
        }
    }
    for i in 0..CLASSES {
        for j in (0..CLASSES).filter(|j| *j != i) {
            out.push((c(i), c(j), EdgeKind::Has));
            out.push((c(i), c(j), EdgeKind::Inherits));
        }
    }
    out
}

/// The touches a function may make: it reads or writes one of `PLACES`.
const TOUCHES: [(bool, usize); 4] = [(false, 0), (true, 0), (false, 1), (true, 1)];

/// A reading: the arrows of `candidates` whose bit is set, and for each function the touches of
/// `TOUCHES` whose bit is set. What a function takes and gives its signature says as its edges do.
fn reading(arrows: &[bool], touches: &[u8]) -> Graph {
    let edges: Vec<Edge> = candidates()
        .into_iter()
        .zip(arrows)
        .filter(|(_, on)| **on)
        .map(|((src, dst, kind), _)| Edge { kind, ..calls(&src, &dst) })
        .collect();
    let short = |id: &str| id.rsplit('.').next().unwrap().to_string();
    let mut nodes: Vec<Node> = (0..CLASSES)
        .map(|i| Node { kind: NodeKind::Class, ..function(&format!("m.C{i}")) })
        .chain((0..FUNCTIONS).map(|i| function(&format!("m.f{i}"))))
        .map(|n| Node { name: short(&n.id), ..n })
        .collect();
    for (i, n) in nodes.iter_mut().filter(|n| n.kind == NodeKind::Function).enumerate() {
        let id = n.id.clone();
        let to = |kind: EdgeKind| edges.iter().filter(|e| e.src == id && e.kind == kind).map(|e| short(&e.dst)).collect::<Vec<_>>();
        n.params =
            to(EdgeKind::Takes).into_iter().map(|t| atlas_engine::Param { name: t.to_lowercase(), ty: t, ..Default::default() }).collect();
        let gives = to(EdgeKind::Returns);
        n.returns = (!gives.is_empty()).then(|| atlas_engine::Returns { written: gives.join(" | "), ..Default::default() });
        n.touches = TOUCHES
            .iter()
            .enumerate()
            .filter(|(k, _)| touches[i] >> k & 1 == 1)
            .map(|(k, (w, p))| touch(if *w { Dir::Write } else { Dir::Read }, *p, k as u32 + 1))
            .collect();
    }
    Graph { nodes, edges, ..Default::default() }
}

/// The arrows directly into and out of node `id` that make it what it is, worked out on the reading
/// alone: of a type, what takes, gives, changes, builds, holds or extends it and what it holds and
/// extends; of a function, what it takes, gives and changes, and what its own calls do at places.
/// What a function calls or builds is a step of its body, no part of its arrow.
fn direct(g: &Graph, id: &str) -> BTreeSet<String> {
    let class = g.node(id).is_some_and(|n| n.kind == NodeKind::Class);
    let typing = |k: EdgeKind| !matches!(k, EdgeKind::Calls);
    let own = |k: EdgeKind| matches!(k, EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Mutates);
    let mut out: BTreeSet<String> = g
        .edges
        .iter()
        .filter(|e| if class { typing(e.kind) && (e.src == id || e.dst == id) } else { own(e.kind) && e.src == id })
        .map(|e| if e.src == id { format!("→ {:?} {}", e.kind, e.dst) } else { format!("← {:?} {}", e.kind, e.src) })
        .collect();
    out.extend(g.node(id).into_iter().flat_map(|n| &n.touches).map(|t| format!("{:?} {}", t.dir, t.artifact)));
    out
}

/// The functions that touch each place, and which way.
fn touched(g: &Graph) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for n in &g.nodes {
        for t in &n.touches {
            out.entry(format!("w:File:{}", t.artifact)).or_default().insert(format!("{} {:?}", n.id, t.dir));
        }
    }
    out
}

fn bits(n: usize) -> impl Strategy<Value = Vec<bool>> {
    proptest::collection::vec(any::<bool>(), n)
}

fn touch_bits() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(0u8..16, FUNCTIONS)
}

proptest! {
    /// Between two readings of the same nodes, a node is changed exactly when its direct arrows
    /// differ, and a place exactly when the functions that touch it, or the way they do, differ: a
    /// change anywhere else, however near, leaves it as it was. A place written is part of a function's
    /// arrow as a type it gives is.
    #[test]
    fn a_node_is_changed_exactly_when_its_direct_arrows_differ(
        a in bits(candidates().len()), flip in bits(candidates().len()), ta in touch_bits(), tflip in touch_bits()
    ) {
        let b: Vec<bool> = a.iter().zip(&flip).map(|(x, f)| x ^ f).collect();
        let tb: Vec<u8> = ta.iter().zip(&tflip).map(|(x, f)| x ^ f).collect();
        let (ga, gb) = (reading(&a, &ta), reading(&b, &tb));
        let (_, d) = atlas_engine::diff::union("m", vec![ga.clone()], vec![gb.clone()]);
        for n in &gb.nodes {
            let changed = direct(&ga, &n.id) != direct(&gb, &n.id);
            prop_assert_eq!(d.nodes.get(&n.id).copied(), changed.then_some(Status::Changed), "{}: {:?} → {:?}", n.id, direct(&ga, &n.id), direct(&gb, &n.id));
        }
        let (pa, pb) = (touched(&ga), touched(&gb));
        for place in pa.keys().chain(pb.keys()) {
            let expected = match (pa.get(place), pb.get(place)) {
                (None, Some(_)) => Some(Status::Added),
                (Some(_), None) => Some(Status::Removed),
                (x, y) if x != y => Some(Status::Changed),
                _ => None,
            };
            prop_assert_eq!(d.places.get(place).copied(), expected, "{}", place);
        }
    }

    /// A node renamed, its relations kept, is one node under a new name: the union marks it alone,
    /// and says what it was called.
    #[test]
    fn a_renamed_node_is_one_node(a in bits(candidates().len()), ta in touch_bits(), which in 0..CLASSES + FUNCTIONS) {
        let ga = reading(&a, &ta);
        let old = ga.nodes[which].id.clone();
        let new = old.replace(".C", ".K").replace(".f", ".g");
        let swap = |s: &str| if s == old { new.clone() } else { s.to_string() };
        let mut gb = ga.clone();
        for n in &mut gb.nodes {
            if n.id == old {
                n.id = new.clone();
                n.name = new.rsplit('.').next().unwrap().into();
            }
            for p in &mut n.params {
                p.ty = p.ty.replace(&old.replace("m.", ""), &new.replace("m.", ""));
            }
            if let Some(r) = n.returns.as_mut() {
                r.written = r.written.replace(&old.replace("m.", ""), &new.replace("m.", ""));
            }
        }
        for e in &mut gb.edges {
            (e.src, e.dst) = (swap(&e.src), swap(&e.dst));
        }
        let (_, d) = atlas_engine::diff::union("m", vec![ga], vec![gb]);
        prop_assert_eq!(d.renamed.clone(), BTreeMap::from([(new.clone(), old.clone())]));
        prop_assert_eq!(d.nodes.clone(), BTreeMap::from([(new, Status::Changed)]));
        prop_assert!(d.edges.is_empty() && d.places.is_empty(), "{:?}", d);
    }
}

// ---------- Reach and gluing ----------

/// A graph of up to ten functions and classes with up to twenty-five lines between them, of every
/// kind that leads somewhere and of some that do not (`Takes`), and a touch of one of `PLACES` on
/// some of them.
fn tangles() -> impl Strategy<Value = Graph> {
    (2usize..10).prop_flat_map(|n| {
        let kinds = prop_oneof![
            Just(EdgeKind::Calls),
            Just(EdgeKind::Instantiates),
            Just(EdgeKind::Requests),
            Just(EdgeKind::Contains),
            Just(EdgeKind::Takes)
        ];
        (
            proptest::collection::vec((any::<bool>(), 0..PLACES.len() * 3, any::<bool>()), n),
            proptest::collection::vec((0..n, 0..n, kinds), 0..25),
        )
            .prop_map(|(nodes, lines)| {
                let nodes = nodes
                    .iter()
                    .enumerate()
                    .map(|(i, (class, at, w))| Node {
                        kind: if *class { NodeKind::Class } else { NodeKind::Function },
                        touches: (*at < PLACES.len()).then(|| touch(if *w { Dir::Write } else { Dir::Read }, *at, 1)).into_iter().collect(),
                        ..function(&format!("m.n{i}"))
                    })
                    .collect();
                let edges =
                    lines.iter().map(|(a, b, kind)| Edge { kind: *kind, ..calls(&format!("m.n{a}"), &format!("m.n{b}")) }).collect();
                Graph { nodes, edges, ..Default::default() }
            })
    })
}

/// A program of up to three functions and a library of up to eight, most of the library's touching
/// one of `PLACES`. Most calls lead from the program into the library and deeper into it; some lead back
/// (a callback the library runs), and the library's functions call one another in cycles too.
fn libraries() -> impl Strategy<Value = Graph> {
    (1usize..4, 2usize..10).prop_flat_map(|(p, l)| {
        let n = p + l;
        (proptest::collection::vec((0..PLACES.len() + 1, any::<bool>()), l), proptest::collection::vec((0..n, 0..n, 0..4u8), n..3 * n))
            .prop_map(move |(touches, lines)| {
                let id = |i: usize| if i < p { format!("app.p{i}") } else { format!("lib.l{}", i - p) };
                let mut nodes: Vec<Node> = (0..p).map(|i| Node { module: "app".into(), ..function(&id(i)) }).collect();
                nodes.extend(touches.iter().enumerate().map(|(i, (at, w))| Node {
                    module: "lib".into(),
                    library: true,
                    touches: (*at < PLACES.len()).then(|| touch(if *w { Dir::Write } else { Dir::Read }, *at, 1)).into_iter().collect(),
                    ..function(&id(p + i))
                }));
                // three of four lines lead deeper, from the lower index to the higher
                let edges: Vec<Edge> = lines
                    .iter()
                    .map(|&(a, b, back)| if back > 0 { (a.min(b), a.max(b)) } else { (a, b) })
                    .filter(|(a, b)| a != b)
                    .map(|(a, b)| calls(&id(a), &id(b)))
                    .collect();
                for n in &mut nodes {
                    n.sites =
                        edges.iter().filter(|e| e.src == n.id).map(|e| Site { to: e.dst.clone(), line: 2, ..Default::default() }).collect();
                }
                Graph { nodes, edges, ..Default::default() }
            })
    })
}

/// The transitive closure of the lines `step` keeps, by Warshall: `far[a][b]` when a path of such
/// lines leads from `a` to `b`.
fn closure(g: &Graph, step: impl Fn(&Edge) -> bool) -> Vec<Vec<bool>> {
    let at: BTreeMap<&str, usize> = g.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let n = g.nodes.len();
    let mut far = vec![vec![false; n]; n];
    for e in g.edges.iter().filter(|e| step(e)) {
        if let (Some(&a), Some(&b)) = (at.get(e.src.as_str()), at.get(e.dst.as_str())) {
            far[a][b] = true;
        }
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                far[i][j] |= far[i][k] && far[k][j];
            }
        }
    }
    far
}

/// The node `i` and every node a path from it leads to.
fn row(g: &Graph, far: &[Vec<bool>], i: usize) -> BTreeSet<String> {
    g.nodes.iter().enumerate().filter(|(j, _)| *j == i || far[i][*j]).map(|(_, n)| n.id.clone()).collect()
}

/// The places a function reaches: what everything it can run touches.
fn places_reached(g: &Graph, id: &str) -> BTreeSet<String> {
    g.reach(id).iter().filter_map(|r| g.node(r)).flat_map(|n| &n.touches).map(|t| format!("{:?} {}", t.dir, t.artifact)).collect()
}

proptest! {
    /// What a route can run is the transitive closure of its steps: the calls, the classes it builds,
    /// the handlers it requests and a class's methods. Its fiber of calls and builds is the closure of
    /// those alone.
    #[test]
    fn reach_is_the_closure_of_the_steps(g in tangles()) {
        let class = |id: &str| g.node(id).is_some_and(|n| n.kind == NodeKind::Class);
        let runs = closure(&g, |e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Requests) || (e.kind == EdgeKind::Contains && class(&e.src)));
        let calls = closure(&g, |e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates));
        for (i, n) in g.nodes.iter().enumerate() {
            prop_assert_eq!(g.reach(&n.id).into_iter().collect::<BTreeSet<_>>(), row(&g, &runs, i), "{}", n.id);
            let fiber: BTreeSet<String> = reach_from(&g, &n.id).apply(&g).nodes.into_iter().map(|n| n.id).collect();
            prop_assert_eq!(fiber, row(&g, &calls, i), "{}", n.id);
        }
    }

    /// Gluing what lies beyond a library's border into the function at the border keeps what every
    /// function of the program reaches at places, however many calls deep the library is read.
    #[test]
    fn gluing_keeps_what_the_program_reaches(g in libraries(), rounds in 1usize..3) {
        let glued = reached(&g, rounds).apply(&g);
        for n in g.nodes.iter().filter(|n| !n.library) {
            prop_assert_eq!(places_reached(&glued, &n.id), places_reached(&g, &n.id), "{} at {} rounds", n.id, rounds);
        }
    }
}
