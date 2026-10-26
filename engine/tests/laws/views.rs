//! The laws of views, each a function of a graph and the sets it is asked about, that says where the
//! two routes of the law part. `laws.rs` asks them of the graphs `proptest` draws, `sample.rs` of the
//! graphs of real programs at sites drawn from a seed: one law, two ways to look for where it breaks.

use std::collections::{BTreeMap, BTreeSet};

use atlas_engine::view::{by_module, Compose, Fiber, Quotient, View};
use atlas_engine::{Graph, Node, NodeKind};

/// A graph as the facts a law compares: its nodes, and its edges with the number each stands for.
pub fn facts(g: &Graph) -> (Vec<String>, Vec<(String, String, String, u32)>) {
    let mut nodes: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
    let mut edges: Vec<_> =
        g.edges.iter().map(|e| (e.src.clone(), e.dst.clone(), format!("{:?}", e.kind), e.weight.unwrap_or(1))).collect();
    nodes.sort();
    edges.sort();
    (nodes, edges)
}

pub fn then(first: impl View + 'static, second: impl View + 'static) -> Compose {
    Compose(vec![Box::new(first), Box::new(second)])
}

/// One node per top-level package: a coarser quotient than `by_module`.
pub fn by_package() -> impl View {
    Quotient(|n: &Node| {
        if n.kind == NodeKind::External {
            return None;
        }
        let package = n.module.split('.').next().unwrap_or(&n.module).to_string();
        Some(Node { id: package.clone(), kind: NodeKind::Module, module: package, ..Default::default() })
    })
}

/// The lines of a quotient by `by_module`, counted on the fine graph: for each pair of different
/// modules and each kind, the fine lines between them whose two ends satisfy `keep`. What
/// `by_module` sends nowhere (an external node, an end the graph does not hold) is no end.
pub fn crossing(g: &Graph, keep: impl Fn(&Node) -> bool) -> BTreeMap<(String, String, String), u32> {
    let mut out = BTreeMap::new();
    for e in &g.edges {
        let (Some(a), Some(b)) = (g.node(&e.src), g.node(&e.dst)) else { continue };
        if a.kind == NodeKind::External || b.kind == NodeKind::External {
            continue;
        }
        if a.module != b.module && keep(a) && keep(b) {
            *out.entry((a.module.clone(), b.module.clone(), format!("{:?}", e.kind))).or_insert(0) += e.weight.unwrap_or(1);
        }
    }
    out
}

pub fn weights(g: &Graph) -> BTreeMap<(String, String, String), u32> {
    g.edges.iter().map(|e| ((e.src.clone(), e.dst.clone(), format!("{:?}", e.kind)), e.weight.unwrap_or(1))).collect()
}

fn holds<T: PartialEq + std::fmt::Debug>(law: &str, a: T, b: T) -> Result<(), String> {
    if a == b {
        Ok(())
    } else {
        let (a, b) = (format!("{a:?}"), format!("{b:?}"));
        let cut = |s: &str| s.chars().take(600).collect::<String>();
        Err(format!("{law}\n  one way:   {}\n  the other: {}", cut(&a), cut(&b)))
    }
}

fn keep(set: &BTreeSet<String>) -> impl Fn(&Node) -> bool + 'static {
    let set = set.clone();
    move |n: &Node| set.contains(&n.id)
}

/// Two fibers commute, and both are the fiber of the conjunction; a fiber taken twice is the fiber
/// taken once.
pub fn fibers_commute(g: &Graph, p: &BTreeSet<String>, q: &BTreeSet<String>) -> Result<(), String> {
    let ab = then(Fiber(keep(p)), Fiber(keep(q))).apply(g);
    let ba = then(Fiber(keep(q)), Fiber(keep(p))).apply(g);
    let both: BTreeSet<String> = p.intersection(q).cloned().collect();
    holds("fibers commute", facts(&ab), facts(&ba))?;
    holds("two fibers are the fiber of the conjunction", facts(&ab), facts(&Fiber(keep(&both)).apply(g)))?;
    let once = Fiber(keep(p)).apply(g);
    holds("a fiber is idempotent", facts(&Fiber(keep(p)).apply(&once)), facts(&once))
}

/// Coarsening in two steps is coarsening in one: the classes of packages over the classes of modules
/// are the classes of packages, and each line between two packages counts the fine lines it stands
/// for, whichever way it was reached. A quotient is idempotent on its image.
pub fn quotients_compose(g: &Graph) -> Result<(), String> {
    holds("quotients compose", facts(&then(by_module(), by_package()).apply(g)), facts(&by_package().apply(g)))?;
    holds("a quotient is idempotent on its image", facts(&by_module().apply(&by_module().apply(g))), facts(&by_module().apply(g)))
}

/// The closed picture is the pushforward of the fine one: a line inside one block is gone, and the
/// lines of one kind between two blocks are one bundle that counts them. Counted on the fine graph,
/// the bundles carry every fine line that crosses a border, and no other.
pub fn a_quotient_counts_the_lines_that_cross_its_borders(g: &Graph) -> Result<(), String> {
    let q = by_module().apply(g);
    let inside: Vec<_> = q.edges.iter().filter(|e| e.src == e.dst).map(|e| e.src.clone()).collect();
    holds("a line inside a block is no line", inside, vec![])?;
    holds("the bundles count the fine lines that cross", weights(&q), crossing(g, |_| true))
}

/// A predicate that depends only on a node's module never cuts a module in two, so filtering the fine
/// graph and quotienting gives what quotienting and filtering the classes gives.
pub fn a_saturated_fiber_commutes(g: &Graph, modules: &BTreeSet<String>) -> Result<(), String> {
    let (a, b) = (modules.clone(), modules.clone());
    let fiber_first = then(Fiber(move |n: &Node| a.contains(&n.module)), by_module()).apply(g);
    let quotient_first = then(by_module(), Fiber(move |n: &Node| b.contains(&n.module))).apply(g);
    holds("a saturated fiber commutes with the quotient", facts(&fiber_first), facts(&quotient_first))
}

/// Whatever the predicate, fiber first counts the fine lines whose two ends it keeps, and quotient
/// first, with the predicate's image on the classes, the fine lines between two classes it keeps a
/// member of. The two agree whenever the predicate is saturated, and the first never counts more
/// than the second.
pub fn a_fiber_before_a_quotient_counts_only_what_it_keeps(g: &Graph, kept: &BTreeSet<String>) -> Result<(), String> {
    let classes = |n: &&Node| n.kind != NodeKind::External;
    let image: BTreeSet<String> = g.nodes.iter().filter(classes).filter(|n| kept.contains(&n.id)).map(|n| n.module.clone()).collect();
    let saturated = g.nodes.iter().filter(classes).all(|n| kept.contains(&n.id) == image.contains(&n.module));
    let fiber_first = weights(&then(Fiber(keep(kept)), by_module()).apply(g));
    let quotient_first = weights(&then(by_module(), Fiber(keep(&image))).apply(g));
    holds("fiber first counts the lines whose ends it keeps", &fiber_first, &crossing(g, |n| kept.contains(&n.id)))?;
    holds("quotient first counts the lines between the classes it keeps", &quotient_first, &crossing(g, |n| image.contains(&n.module)))?;
    let more: Vec<_> = fiber_first.iter().filter(|(line, w)| quotient_first.get(*line).map_or(true, |v| *w > v)).collect();
    holds("fiber first never counts more", more, vec![])?;
    if saturated {
        holds("a saturated fiber commutes with the quotient", fiber_first, quotient_first)?;
    }
    Ok(())
}
