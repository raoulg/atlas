//! The laws of views, each computed along two routes that must agree, or shown by the smallest
//! counterexample where `docs/reference/mathematics.md` says they differ. The subjects are graphs
//! made here, never a program: a handful of functions in modules of two packages, so that a failure
//! points at a law and never at what some program happens to contain. Each law holds for every
//! graph `proptest` draws (`PROPTEST_CASES` sets how many, 256 by default); a graph it breaks on is
//! shrunk to the smallest one that still breaks it. The counterexample is a graph written out. The
//! laws themselves are `laws/views.rs`, which `sample.rs` asks of real programs.
//!
//! A view is a fiber (keep where a predicate holds) or a quotient (identify the nodes a map sends to
//! one class). Fibers commute with each other; quotients compose; a fiber and a quotient commute
//! exactly when the fiber's predicate is saturated by the quotient's classes ("Saturation: when the
//! order of a view does not matter"), and otherwise Atlas takes the fiber first.

#[path = "laws/views.rs"]
mod views;

use std::collections::BTreeSet;

use atlas_engine::view::{by_module, Fiber, View};
use atlas_engine::{Edge, EdgeKind, Graph, Node, NodeKind};
use proptest::prelude::*;
use views::{facts, then};

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

/// The node ids of `g` whose bit in `bits` is set.
fn subset(g: &Graph, bits: u16) -> BTreeSet<String> {
    g.nodes.iter().enumerate().filter(|(i, _)| bits >> (i % 16) & 1 == 1).map(|(_, n)| n.id.clone()).collect()
}

proptest! {
    #[test]
    fn fibers_commute_and_meet_in_the_conjunction(g in graphs(), one in any::<u16>(), other in any::<u16>()) {
        views::fibers_commute(&g, &subset(&g, one), &subset(&g, other)).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn quotients_compose(g in graphs()) {
        views::quotients_compose(&g).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn a_quotient_counts_the_lines_that_cross_its_borders(g in graphs()) {
        views::a_quotient_counts_the_lines_that_cross_its_borders(&g).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn a_saturated_fiber_commutes_with_the_quotient(g in graphs(), kept in proptest::collection::btree_set(0..MODULES.len(), 0..=MODULES.len())) {
        let modules: BTreeSet<String> = kept.iter().map(|m| MODULES[*m].to_string()).collect();
        views::a_saturated_fiber_commutes(&g, &modules).map_err(TestCaseError::fail)?;
    }

    #[test]
    fn a_fiber_before_a_quotient_counts_only_what_it_keeps(g in graphs(), bits in any::<u16>()) {
        views::a_fiber_before_a_quotient_counts_only_what_it_keeps(&g, &subset(&g, bits)).map_err(TestCaseError::fail)?;
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
