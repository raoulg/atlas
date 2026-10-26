//! The laws of views, each computed along two routes that must agree, or shown by the smallest
//! counterexample where `docs/reference/mathematics.md` says they differ. The subjects are graphs
//! made here, never a program: a handful of functions in modules of two packages, so that a failure
//! points at a law and never at what some program happens to contain. Each law holds for every
//! graph `proptest` draws (`PROPTEST_CASES` sets how many, 256 by default); a graph it breaks on is
//! shrunk to the smallest one that still breaks it. The counterexample is a graph written out.
//!
//! A view is a fiber (keep where a predicate holds) or a quotient (identify the nodes a map sends to
//! one class). Fibers commute with each other; quotients compose; a fiber and a quotient commute
//! exactly when the fiber's predicate is saturated by the quotient's classes ("Saturation: when the
//! order of a view does not matter"), and otherwise Atlas takes the fiber first.

use std::collections::{BTreeMap, BTreeSet};

use atlas_engine::view::{by_module, Compose, Fiber, Quotient, View};
use atlas_engine::{Edge, EdgeKind, Graph, Node, NodeKind};
use proptest::prelude::*;

fn function(id: &str, private: bool) -> Node {
    let module = id.rsplit_once('.').unwrap().0.to_string();
    Node { id: id.into(), kind: NodeKind::Function, name: id.rsplit('.').next().unwrap().into(), module, private, ..Default::default() }
}

fn calls(src: &str, dst: &str) -> Edge {
    edge(src, dst, EdgeKind::Calls)
}

fn edge(src: &str, dst: &str, kind: EdgeKind) -> Edge {
    Edge { src: src.into(), dst: dst.into(), kind, via: None, weight: None, arms: vec![], uses: false }
}

const MODULES: [&str; 4] = ["a.x", "a.y", "b.z", "b.w"];

/// A graph of up to twelve functions in the four modules of `MODULES`, some private, with up to
/// thirty calls and constructions between them, loops and repeats included.
fn graphs() -> impl Strategy<Value = Graph> {
    (1usize..12, any::<u64>()).prop_flat_map(|(n, _)| {
        (proptest::collection::vec((0..MODULES.len(), any::<bool>()), n), proptest::collection::vec((0..n, 0..n, any::<bool>()), 0..30))
            .prop_map(|(nodes, lines)| {
                let ids: Vec<String> = nodes.iter().enumerate().map(|(i, (m, _))| format!("{}.f{i}", MODULES[*m])).collect();
                let mut g = Graph {
                    nodes: nodes.iter().zip(&ids).map(|((_, private), id)| function(id, *private)).collect(),
                    edges: lines
                        .iter()
                        .map(|(a, b, builds)| edge(&ids[*a], &ids[*b], if *builds { EdgeKind::Instantiates } else { EdgeKind::Calls }))
                        .collect(),
                    ..Default::default()
                };
                g.recount();
                g
            })
    })
}

/// `app.cli` calls into `app.core`, which calls `app.store` and a private helper of its own; the
/// store calls back into the core once, and `lib.text` is called from the cli and from the core.
fn subject() -> Graph {
    let mut g = Graph {
        nodes: [
            ("app.cli.main", false),
            ("app.cli.show", false),
            ("app.core.run", false),
            ("app.core._step", true),
            ("app.store.save", false),
            ("app.store.load", false),
            ("lib.text.fmt", false),
        ]
        .into_iter()
        .map(|(id, private)| function(id, private))
        .collect(),
        edges: [
            ("app.cli.main", "app.core.run"),
            ("app.cli.main", "app.cli.show"),
            ("app.cli.show", "lib.text.fmt"),
            ("app.core.run", "app.core._step"),
            ("app.core._step", "app.store.save"),
            ("app.core.run", "app.store.load"),
            ("app.store.load", "app.core.run"),
            ("app.core.run", "lib.text.fmt"),
        ]
        .into_iter()
        .map(|(a, b)| calls(a, b))
        .collect(),
        ..Default::default()
    };
    g.recount();
    g
}

/// A graph as the facts a law compares: its nodes, and its edges with the number each stands for.
fn facts(g: &Graph) -> (Vec<String>, Vec<(String, String, String, u32)>) {
    let mut nodes: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
    let mut edges: Vec<_> =
        g.edges.iter().map(|e| (e.src.clone(), e.dst.clone(), format!("{:?}", e.kind), e.weight.unwrap_or(1))).collect();
    nodes.sort();
    edges.sort();
    (nodes, edges)
}

fn then(first: impl View + 'static, second: impl View + 'static) -> Compose {
    Compose(vec![Box::new(first), Box::new(second)])
}

/// One node per top-level package: a coarser quotient than `by_module`.
fn by_package() -> impl View {
    Quotient(|n: &Node| {
        let package = n.module.split('.').next().unwrap_or(&n.module).to_string();
        Some(Node { id: package.clone(), kind: NodeKind::Module, module: package, ..Default::default() })
    })
}

/// The lines of a quotient by `by_module`, counted on the fine graph: for each pair of different
/// modules and each kind, the fine lines between them whose two ends satisfy `keep`.
fn crossing(g: &Graph, keep: impl Fn(&Node) -> bool) -> BTreeMap<(String, String, String), u32> {
    let mut out = BTreeMap::new();
    for e in &g.edges {
        let (a, b) = (g.node(&e.src).unwrap(), g.node(&e.dst).unwrap());
        if a.module != b.module && keep(a) && keep(b) {
            *out.entry((a.module.clone(), b.module.clone(), format!("{:?}", e.kind))).or_insert(0) += 1;
        }
    }
    out
}

fn weights(g: &Graph) -> BTreeMap<(String, String, String), u32> {
    g.edges.iter().map(|e| ((e.src.clone(), e.dst.clone(), format!("{:?}", e.kind)), e.weight.unwrap_or(1))).collect()
}

proptest! {
    /// Two fibers commute, and both are the fiber of the conjunction; a fiber taken twice is the
    /// fiber taken once.
    #[test]
    fn fibers_commute_and_meet_in_the_conjunction(g in graphs(), one in any::<u16>(), other in any::<u16>()) {
        let ids: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
        let set = |bits: u16| ids.iter().enumerate().filter(|(i, _)| bits >> (i % 16) & 1 == 1).map(|(_, id)| id.clone()).collect::<BTreeSet<_>>();
        let (p, q) = (set(one), set(other));
        let (pp, qq, pq) = (p.clone(), q.clone(), (p.clone(), q.clone()));
        let ab = then(Fiber(move |n: &Node| pp.contains(&n.id)), Fiber(move |n: &Node| qq.contains(&n.id))).apply(&g);
        let (pp, qq) = (p.clone(), q.clone());
        let ba = then(Fiber(move |n: &Node| qq.contains(&n.id)), Fiber(move |n: &Node| pp.contains(&n.id))).apply(&g);
        let both = Fiber(move |n: &Node| pq.0.contains(&n.id) && pq.1.contains(&n.id)).apply(&g);
        prop_assert_eq!(facts(&ab), facts(&ba), "fibers commute");
        prop_assert_eq!(facts(&ab), facts(&both), "two fibers are the fiber of the conjunction");
        let pp = p.clone();
        let once = Fiber(move |n: &Node| pp.contains(&n.id)).apply(&g);
        let pp = p.clone();
        prop_assert_eq!(facts(&Fiber(move |n: &Node| pp.contains(&n.id)).apply(&once)), facts(&once), "a fiber is idempotent");
    }

    /// Coarsening in two steps is coarsening in one: the classes of packages over the classes of
    /// modules are the classes of packages, and each line between two packages counts the fine lines
    /// it stands for, whichever way it was reached. A quotient is idempotent on its image.
    #[test]
    fn quotients_compose(g in graphs()) {
        prop_assert_eq!(facts(&then(by_module(), by_package()).apply(&g)), facts(&by_package().apply(&g)));
        prop_assert_eq!(facts(&by_module().apply(&by_module().apply(&g))), facts(&by_module().apply(&g)));
    }

    /// The closed picture is the pushforward of the fine one: a line inside one block is gone, and
    /// the lines of one kind between two blocks are one bundle that counts them. Counted on the fine
    /// graph, the bundles carry every fine line that crosses a border, and no other.
    #[test]
    fn a_quotient_counts_the_lines_that_cross_its_borders(g in graphs()) {
        let q = by_module().apply(&g);
        prop_assert!(q.edges.iter().all(|e| e.src != e.dst), "a line inside a block is no line");
        prop_assert_eq!(weights(&q), crossing(&g, |_| true));
    }

    /// A predicate that depends only on a node's module never cuts a module in two, so filtering the
    /// fine graph and quotienting gives what quotienting and filtering the classes gives.
    #[test]
    fn a_saturated_fiber_commutes_with_the_quotient(g in graphs(), kept in proptest::collection::btree_set(0..MODULES.len(), 0..=MODULES.len())) {
        let modules: BTreeSet<String> = kept.iter().map(|m| MODULES[*m].to_string()).collect();
        let (a, b) = (modules.clone(), modules.clone());
        let fiber_first = then(Fiber(move |n: &Node| a.contains(&n.module)), by_module()).apply(&g);
        let quotient_first = then(by_module(), Fiber(move |n: &Node| b.contains(&n.module))).apply(&g);
        prop_assert_eq!(facts(&fiber_first), facts(&quotient_first));
    }

    /// Whatever the predicate, fiber first counts the fine lines whose two ends it keeps, and
    /// quotient first, with the predicate's image on the classes, the fine lines between two classes
    /// it keeps a member of. The two agree whenever the predicate is saturated, and the first never
    /// counts more than the second.
    #[test]
    fn a_fiber_before_a_quotient_counts_only_what_it_keeps(g in graphs(), bits in any::<u16>()) {
        let kept: BTreeSet<String> = g.nodes.iter().enumerate().filter(|(i, _)| bits >> (i % 16) & 1 == 1).map(|(_, n)| n.id.clone()).collect();
        let image: BTreeSet<String> = g.nodes.iter().filter(|n| kept.contains(&n.id)).map(|n| n.module.clone()).collect();
        let saturated = g.nodes.iter().all(|n| kept.contains(&n.id) == image.contains(&n.module));
        let (k, i) = (kept.clone(), image.clone());
        let fiber_first = weights(&then(Fiber(move |n: &Node| k.contains(&n.id)), by_module()).apply(&g));
        let quotient_first = weights(&then(by_module(), Fiber(move |n: &Node| i.contains(&n.id))).apply(&g));
        prop_assert_eq!(&fiber_first, &crossing(&g, |n| kept.contains(&n.id)));
        prop_assert_eq!(&quotient_first, &crossing(&g, |n| image.contains(&n.module)));
        prop_assert!(fiber_first.iter().all(|(line, w)| quotient_first.get(line).map_or(false, |v| w <= v)));
        if saturated {
            prop_assert_eq!(fiber_first, quotient_first);
        }
    }
}

/// Hiding what is private cuts `app.core` in two: `_step` goes, `run` stays. The order is then
/// meaning. Fiber first, the core's bundle to the store counts only `run → load`; quotient first,
/// the class `app.core` is public (it is a module) and keeps the line through `_step` as well.
/// Atlas takes the fiber first, so the reader decides what exists before the quotient groups it.
#[test]
fn an_unsaturated_fiber_does_not_commute_with_the_quotient() {
    let g = subject();
    let public = |n: &Node| !n.private;
    let core_to_store = |g: &Graph| g.edges.iter().find(|e| e.src == "app.core" && e.dst == "app.store").and_then(|e| e.weight);
    let fiber_first = then(Fiber(public), by_module()).apply(&g);
    let quotient_first = then(by_module(), Fiber(public)).apply(&g);
    assert_ne!(facts(&fiber_first), facts(&quotient_first), "the predicate cuts a class, so the orders differ");
    assert_eq!(core_to_store(&fiber_first), Some(1), "fiber first: only the public `run → load` is left to count");
    assert_eq!(core_to_store(&quotient_first), Some(2), "quotient first: the private line was counted before the fiber");
}
