//! Pairs: the same program twice, once as good practice writes it and once with the shortcut
//! people typically take. The clean one must read as the algebra says; the shortcut must show as
//! a shortcut (a place the code does not name, an open question), never as if it were clean and
//! never not at all. What differs between the two pictures is what Atlas lays bare.

use std::path::PathBuf;

use atlas_engine::parts::analyze_repo;
use atlas_engine::{EdgeKind, Graph, Options, World};

fn pair(name: &str) -> Graph {
    analyze_repo(&[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(name)], &Options::default()).unwrap()
}

/// A client that names the paths it asks for is joined to the handlers that serve them, by
/// method and path: its POST to `/api/orders` is not the GET of that path. One that looks its paths up by a key at run time, with an environment
/// variable that may stand in for each, says no path: its requests go to an address the code
/// does not say, and the paths in its table join nothing.
#[test]
fn a_request_meets_the_route_it_names() {
    let g = pair("requests");
    let requests = |side: &str| -> Vec<(String, String)> {
        g.edges.iter().filter(|e| e.kind == EdgeKind::Requests && e.src.starts_with(side)).map(|e| (e.src.clone(), e.dst.clone())).collect()
    };
    let want = [("clean.client.item", "shop.api.item"), ("clean.client.order", "shop.api.order")];
    assert_eq!(requests("clean."), want.map(|(a, b)| (a.to_string(), b.to_string())));
    assert_eq!(requests("shortcut."), [], "a path looked up at run time names no route, and a path in a table is no request");
    for f in ["shortcut.client.item", "shortcut.client.order"] {
        let net = g.node(f).unwrap().touches.iter().find(|t| t.kind == World::Net).cloned();
        assert!(net.as_ref().map_or(false, |t| t.artifact.is_empty() && !t.fixed), "{f} asks an address the code does not say: {net:?}");
    }
}
