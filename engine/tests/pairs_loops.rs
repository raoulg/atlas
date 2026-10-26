//! A loop over the stations of a network, in Python, Rust and TypeScript. A loop is a step of its
//! own: it takes what it goes over and gives each element under the name it binds, so the element
//! is a value the body makes, not one from beyond. On the clean side the loop goes over the network
//! the function is handed; on the shortcut side over a list the module keeps, which the function
//! is not handed. What tells them apart is what the loop's step takes: a parameter, or nothing the
//! function is given.

use atlas_engine::{analyze, Graph, Node, Options, Site};
use std::path::PathBuf;

const LANGUAGES: [&str; 3] = ["python", "rust", "typescript"];

fn graph(language: &str, side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/loops").join(language).join(side);
    analyze(&dir, &Options::default()).unwrap()
}

/// The function that reports every station, `report_all` or `reportAll`.
fn report_all(g: &Graph) -> &Node {
    g.nodes.iter().find(|n| n.name == "report_all" || n.name == "reportAll").expect("the function that reports every station")
}

/// The loop's step in a body: the operation written as the loop's head.
fn each(n: &Node) -> &Site {
    n.sites.iter().find(|s| s.op.starts_with("for station")).unwrap_or_else(|| panic!("a step for the loop in {}: {:?}", n.id, n.sites))
}

/// On both sides the loop gives a station, and the call in its body is handed that station: the
/// element is made by the body, by its loop.
#[test]
fn a_loop_is_a_step_that_gives_each_element() {
    for language in LANGUAGES {
        for side in ["clean", "shortcut"] {
            let g = graph(language, side);
            let f = report_all(&g);
            let loop_ = each(f);
            assert!(
                loop_.gives.iter().any(|h| h.name == "station" && h.ty.ends_with("Station")),
                "{language} {side}: the loop gives each station: {:?}",
                loop_.gives
            );
            let report = f.sites.iter().find(|s| s.to.ends_with("report")).expect("the call that reports one station");
            assert!(
                report.hands.iter().any(|h| h.name == "station"),
                "{language} {side}: report is handed the loop's station: {:?}",
                report.hands
            );
            let at = |s: &Site| f.sites.iter().position(|x| std::ptr::eq(x, s)).unwrap();
            assert!(at(loop_) < at(report), "{language} {side}: the loop's step runs before the calls in its body");
        }
    }
}

/// The two sides are told apart by what the loop takes: on the clean side the network the function
/// is handed, on the shortcut side nothing it is handed.
#[test]
fn a_loop_over_what_the_step_is_handed_takes_it() {
    for language in LANGUAGES {
        let takes_a_parameter = |side: &str| {
            let g = graph(language, side);
            let f = report_all(&g);
            each(f).hands.iter().any(|h| f.params.iter().any(|p| p.name == h.name))
        };
        assert!(takes_a_parameter("clean"), "{language}: the loop goes over the network the function is handed");
        assert!(!takes_a_parameter("shortcut"), "{language}: the loop goes over the module's list, which the function is not handed");
    }
}
