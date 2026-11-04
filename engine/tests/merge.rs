//! Parts merged into one graph: the merge of no parts is the empty graph, the calls the parts did
//! not follow are counted by name over all of them, and a file two parts keep lines of keeps every
//! line either shows.

use std::collections::{BTreeMap, BTreeSet};

use atlas_engine::{diff, parts, Graph, Lang, Part, Stats};
use proptest::prelude::*;

/// Counts of names, the most first and equal counts by name.
fn ranked(counts: &BTreeMap<String, u32>) -> Vec<(String, u32)> {
    let mut top: Vec<(String, u32)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top
}

/// A part that did not follow the calls `counts` names, as many times as each says.
fn part(counts: &BTreeMap<String, u32>) -> Graph {
    Graph {
        stats: Stats { unresolved_calls_total: counts.values().sum(), unresolved_calls_top: ranked(counts), ..Default::default() },
        ..Default::default()
    }
}

/// A part that keeps `text` of the file `lib.py`.
fn keeping(text: String) -> Graph {
    Graph { files: BTreeMap::from([("lib.py".to_string(), text)]), ..Default::default() }
}

/// The first `len` lines of `source`, blank where `shown` says the part shows nothing.
fn shown_of(source: &[String], shown: &[bool], len: usize) -> String {
    source.iter().zip(shown).take(len).map(|(l, s)| if *s { l.as_str() } else { "" }).collect::<Vec<_>>().join("\n")
}

fn json(g: &Graph) -> serde_json::Value {
    serde_json::to_value(g).unwrap()
}

/// Law: the merge of no parts is the empty graph, the unit of merging; a side of a comparison
/// that reads no parts is one.
#[test]
fn no_parts_merge_to_the_empty_graph() {
    let mut empty = Graph { root: "repo".into(), ..Default::default() };
    empty.recount();
    assert_eq!(json(&parts::merge("repo", vec![])), json(&empty));
    assert_eq!(json(&parts::joined("repo", vec![])), json(&empty));
    let (union, _) = diff::union("repo", vec![], vec![]);
    assert_eq!(json(&union), json(&empty));
}

fn counts() -> impl Strategy<Value = BTreeMap<String, u32>> {
    proptest::collection::btree_map("[a-f]", 1u32..40, 0..6)
}

proptest! {
    /// Law: the merged top of the calls not followed is the top of the parts' counts summed by
    /// name, and merging in two steps counts as merging at once.
    #[test]
    fn the_merged_top_is_the_top_of_the_summed_counts(parts_counts in proptest::collection::vec(counts(), 0..5)) {
        let mut summed: BTreeMap<String, u32> = BTreeMap::new();
        for (name, n) in parts_counts.iter().flatten() {
            *summed.entry(name.clone()).or_default() += n;
        }
        let merged = parts::merge("repo", parts_counts.iter().map(part).collect());
        prop_assert_eq!(&merged.stats.unresolved_calls_top, &ranked(&summed));
        prop_assert_eq!(merged.stats.unresolved_calls_total, summed.values().sum::<u32>());

        let (first, rest) = parts_counts.split_at(parts_counts.len() / 2);
        let halves = vec![
            parts::merge("repo", first.iter().map(part).collect()),
            parts::merge("repo", rest.iter().map(part).collect()),
        ];
        prop_assert_eq!(&parts::merge("repo", halves).stats.unresolved_calls_top, &merged.stats.unresolved_calls_top);
    }

    /// Law: of two texts of one file, each blank where its part shows nothing and each as long as
    /// its part read it, the merge shows every line either shows, where it stands in the file,
    /// whichever comes first.
    #[test]
    fn a_shared_file_keeps_every_line_either_shows(
        (source, a, b, la, lb) in (1usize..30).prop_flat_map(|n| (
            Just((1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>()),
            proptest::collection::vec(any::<bool>(), n),
            proptest::collection::vec(any::<bool>(), n),
            1..=n,
            1..=n,
        ))
    ) {
        let merge = |x: String, y: String| parts::merge("repo", vec![keeping(x), keeping(y)]).files["lib.py"].clone();
        let merged = merge(shown_of(&source, &a, la), shown_of(&source, &b, lb));
        let lines: Vec<&str> = merged.split('\n').collect();
        prop_assert_eq!(lines.len(), la.max(lb));
        for (i, line) in lines.iter().enumerate() {
            let shows = (i < la && a[i]) || (i < lb && b[i]);
            prop_assert_eq!(*line, if shows { source[i].as_str() } else { "" }, "line {}", i + 1);
        }
        prop_assert_eq!(merged, merge(shown_of(&source, &b, lb), shown_of(&source, &a, la)));
    }
}

/// A part's graph as it is read: in one language, each of its parts in that language.
fn read_in(language: Lang, parts: usize) -> Graph {
    let part = |i: usize| Part {
        id: format!("p{i}"),
        language,
        dir: String::new(),
        leaves: atlas_engine::language::leaves(language),
        structural: atlas_engine::language::structural(language),
    };
    Graph { language: language.into(), parts: (0..parts).map(part).collect(), ..Default::default() }
}

/// Law: a language is written by its name, and read back from it.
#[test]
fn a_language_is_written_by_its_name() {
    for l in Lang::ALL {
        assert_eq!(serde_json::to_value(l).unwrap(), l.name());
        assert_eq!(serde_json::from_value::<Lang>(serde_json::json!(l.name())).unwrap(), l);
    }
}

proptest! {
    /// Law: a repository's graph is the coproduct of its parts', each read in one language, so its
    /// languages are the image of theirs: the merge's are the union of the merged graphs', in two
    /// steps as at once. The JSON writes the one name where there is one and none otherwise, and the
    /// graph read back from it has the languages it was written with.
    #[test]
    fn the_merged_languages_are_the_union_of_the_parts(read in proptest::collection::vec((proptest::sample::select(Lang::ALL.to_vec()), 1usize..3), 0..5)) {
        let graphs: Vec<Graph> = read.iter().map(|(l, n)| read_in(*l, *n)).collect();
        let union: BTreeSet<Lang> = read.iter().map(|(l, _)| *l).collect();
        let merged = parts::merge("repo", graphs.clone());
        prop_assert_eq!(&merged.language.0, &union);

        let (first, rest) = graphs.split_at(graphs.len() / 2);
        let halves = vec![parts::merge("repo", first.to_vec()), parts::merge("repo", rest.to_vec())];
        prop_assert_eq!(&parts::merge("repo", halves).language, &merged.language);

        for g in graphs.iter().chain([&merged]) {
            let written = json(g);
            let one = (g.language.0.len() == 1).then(|| g.language.0.first().unwrap().name());
            prop_assert_eq!(written.get("language").and_then(|v| v.as_str()), one);
            let back: Graph = serde_json::from_value(written).unwrap();
            prop_assert_eq!(&back.language, &g.language);
        }
    }
}

/// A part holding the functions `ids` as part `i` reads them (its doc says which), with a line from
/// each to the next.
fn holding(i: usize, ids: &[String]) -> Graph {
    let node = |id: &String| atlas_engine::Node {
        id: id.clone(),
        kind: atlas_engine::NodeKind::Function,
        doc: format!("read by {i}"),
        ..Default::default()
    };
    let line = |w: &[String]| atlas_engine::Edge {
        src: w[0].clone(),
        dst: w[1].clone(),
        kind: atlas_engine::EdgeKind::Calls,
        via: Some(format!("by {i}")),
        weight: None,
        arms: vec![],
        uses: false,
    };
    Graph { nodes: ids.iter().map(node).collect(), edges: ids.windows(2).map(line).collect(), ..Default::default() }
}

proptest! {
    /// Law: a repository's graph is its parts' glued along what they share (`mathematics.md`,
    /// Coproducts), and an id names one node: the merge holds each id the parts hold once, as the
    /// first part that holds it reads it, with the lines out of it that part draws; in two steps as
    /// at once.
    #[test]
    fn a_merge_holds_each_id_once(held in proptest::collection::vec(proptest::collection::btree_set("[a-e]", 0..5), 0..5)) {
        let graphs: Vec<Graph> = held.iter().enumerate().map(|(i, ids)| holding(i, &ids.iter().cloned().collect::<Vec<_>>())).collect();
        let merged = parts::merge("repo", graphs.clone());
        let ids: Vec<&str> = merged.nodes.iter().map(|n| n.id.as_str()).collect();
        let all: BTreeSet<&String> = held.iter().flatten().collect();
        prop_assert_eq!(ids.len(), all.len(), "{:?}", ids);
        for n in &merged.nodes {
            let first = held.iter().position(|ids| ids.contains(&n.id)).unwrap();
            prop_assert_eq!(&n.doc, &format!("read by {first}"));
            for e in merged.edges.iter().filter(|e| e.src == n.id) {
                prop_assert_eq!(e.via.clone(), Some(format!("by {first}")));
            }
        }
        let (first, rest) = graphs.split_at(graphs.len() / 2);
        let halves = vec![parts::merge("repo", first.to_vec()), parts::merge("repo", rest.to_vec())];
        prop_assert_eq!(json(&parts::merge("repo", halves)), json(&merged));
    }
}
