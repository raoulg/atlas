//! The steps of a body as a string diagram: each call is an arrow, and the values between them
//! are its wires. Composition is call order, and `f(g(x))` is `f ∘ g`: what the inner call gives
//! is an input of the outer one, and what the inner call is handed is its own input, never the
//! outer call's.
//!
//! The readers record each call with where it is written and each value it is handed with where
//! that is written; what follows is the same in every language.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Held, Node, NodeKind, Site};

/// Wire the steps of every body: a call written inside another call hands it its result, under
/// the name the body gives it or, without one, under the step's own (`#n`, unique in the graph);
/// a library's operation that only turns a value of one type into a value of the same type is an
/// identity, and the value it names is the one the step before it makes.
pub fn wire(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let gives_class = classes_given(nodes, edges);
    let mut next = 0;
    for id in nodes.keys().cloned().collect::<Vec<_>>() {
        let mut sites = std::mem::take(&mut nodes[&id].sites);
        nest(&mut sites, &gives_class, &mut next);
        drop_identities(&mut sites);
        nodes[&id].sites = sites;
    }
}

/// The class a step gives when the body does not name its result: the class a constructor builds,
/// or the class a function's signature returns.
fn classes_given(nodes: &IndexMap<String, Node>, edges: &[Edge]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for e in edges.iter().filter(|e| e.kind == EdgeKind::Returns) {
        if nodes.get(&e.dst).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External)) {
            out.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
        }
    }
    for n in nodes.values().filter(|n| n.kind == NodeKind::Class) {
        out.insert(n.id.clone(), n.id.clone());
    }
    out
}

fn inside(inner: (u32, u32), outer: (u32, u32)) -> bool {
    inner != outer && outer.0 <= inner.0 && inner.1 <= outer.1
}

fn written(s: &Site) -> bool {
    s.span != (0, 0)
}

/// Each step written inside another hands it its result; the outer step is no longer handed what
/// the inner one is.
fn nest(sites: &mut [Site], gives_class: &HashMap<String, String>, next: &mut usize) {
    // the call each step is written in: the smallest other step around it
    let around: Vec<Option<usize>> = (0..sites.len())
        .map(|i| {
            (0..sites.len())
                .filter(|&j| j != i && written(&sites[i]) && written(&sites[j]) && inside(sites[i].span, sites[j].span))
                .min_by_key(|&j| sites[j].span.1 - sites[j].span.0)
        })
        .collect();
    for i in 0..sites.len() {
        let Some(outer) = around[i] else { continue };
        // the result without a name is named by its step
        let name = format!("#{next}");
        *next += 1;
        let s = &mut sites[i];
        if !s.gives.iter().any(|g| !g.name.is_empty()) {
            if s.gives.is_empty() {
                let ty = if s.to.is_empty() { String::new() } else { gives_class.get(&s.to).cloned().unwrap_or_default() };
                s.gives.push(Held::new(ty, name.clone()));
            }
            for g in s.gives.iter_mut() {
                g.name = name.clone();
            }
        }
        let made: Vec<Held> = s.gives.clone();
        let o = &mut sites[outer];
        for h in made {
            if !o.hands.contains(&h) {
                o.hands.push(h);
            }
        }
    }
    // what a step written inside another is handed is that step's, not the outer one's
    for j in 0..sites.len() {
        let inner: Vec<(u32, u32)> = (0..sites.len()).filter(|&i| i != j && written(&sites[i]) && written(&sites[j]) && inside(sites[i].span, sites[j].span)).map(|i| sites[i].span).collect();
        if inner.is_empty() {
            continue;
        }
        sites[j].hands.retain(|h| h.at == 0 || !inner.iter().any(|(a, b)| *a <= h.at && h.at < *b));
    }
}

/// A library's operation handed one value and giving a value of the same type, either the result of
/// the step before it or the value it is named for, is an identity (`option.ok_or_else(..)`,
/// `x = x.copy()`): the arrow is not drawn, and the step before it gives what it names.
fn drop_identities(sites: &mut Vec<Site>) {
    let mut gone = vec![];
    for b in 0..sites.len() {
        let s = &sites[b];
        let ([h], [g]) = (s.hands.as_slice(), s.gives.as_slice()) else { continue };
        if !s.to.is_empty() || g.ty.is_empty() || g.ty != h.ty || !(h.name.starts_with('#') || h.name == g.name) {
            continue;
        }
        let (h, g) = (h.clone(), g.clone());
        if let Some(maker) = sites.iter().position(|x| x.gives.contains(&h)) {
            for x in sites[maker].gives.iter_mut().filter(|x| **x == h) {
                *x = g.clone();
            }
        }
        gone.push(b);
    }
    for b in gone.into_iter().rev() {
        sites.remove(b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(to: &str, op: &str, span: (u32, u32), hands: Vec<Held>, gives: Vec<Held>) -> Site {
        Site { to: to.into(), op: op.into(), span, hands, gives, ..Default::default() }
    }

    fn at(ty: &str, name: &str, at: u32) -> Held {
        Held { ty: ty.into(), name: name.into(), at }
    }

    /// `lang = detect(opts).ok_or_else(..)`: `detect` is handed `opts` and gives `lang`; the
    /// unwrapping between them is an identity.
    #[test]
    fn a_nested_call_hands_its_result_on() {
        let mut sites = vec![
            site("detect", "", (10, 22), vec![at("Options", "opts", 17)], vec![]),
            site("", "ok_or_else", (10, 40), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
        ];
        let gives_class = HashMap::from([("detect".to_string(), "Language".to_string())]);
        nest(&mut sites, &gives_class, &mut 0);
        assert_eq!(sites[1].hands, [Held::new("Language", "#0")], "the outer call is handed the inner one's result, not its arguments");
        drop_identities(&mut sites);
        assert_eq!(sites.len(), 1, "an identity is not drawn");
        assert_eq!((sites[0].hands.clone(), sites[0].gives.clone()), (vec![Held::new("Options", "opts")], vec![Held::new("Language", "lang")]));
    }

    /// `f(g(x), y)`: `g` takes `x`, `f` takes what `g` gives and `y`.
    #[test]
    fn composition_is_wired() {
        let mut sites = vec![site("g", "", (2, 6), vec![at("X", "x", 4)], vec![]), site("f", "", (0, 10), vec![at("X", "x", 4), at("Y", "y", 8)], vec![])];
        nest(&mut sites, &HashMap::from([("g".to_string(), "Z".to_string())]), &mut 0);
        assert_eq!(sites[0].gives, [Held::new("Z", "#0")]);
        assert_eq!(sites[1].hands, [Held::new("Y", "y"), Held::new("Z", "#0")]);
    }

    /// `lang = detect(opts).unwrap()` read by a language that names the unwrapped value already:
    /// the arrow from `lang` to `lang` is the identity.
    #[test]
    fn an_identity_on_a_named_value_is_not_drawn() {
        let mut sites = vec![
            site("detect", "", (10, 22), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
            site("", "unwrap", (10, 31), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
        ];
        nest(&mut sites, &HashMap::new(), &mut 0);
        drop_identities(&mut sites);
        assert_eq!(sites.iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), ["detect"]);
    }
}
