//! A repository of several parts: each manifest roots one, read in its own language, and a
//! route one part serves joins it to the part that names the route.

use std::path::PathBuf;

use atlas_engine::parts::{analyze_parts, analyze_repo, roots};
use atlas_engine::{EdgeKind, EntryKind, Graph, Options};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-parts")
}

fn shop() -> Graph {
    analyze_repo(&[fixture()], &Options::default()).expect("the two-part fixture parses")
}

fn label(g: &Graph, id: &str) -> String {
    g.node(id).and_then(|n| n.entry.as_ref()).map(|e| e.label.clone()).unwrap_or_default()
}

/// A manifest roots a part in its language; tests' fixtures and a workspace's member crates
/// are not parts of their own.
#[test]
fn every_manifest_roots_a_part() {
    let found: Vec<(String, &str)> = roots(&fixture(), &Options::default()).into_iter().map(|(d, l)| (d.file_name().unwrap().to_string_lossy().to_string(), l)).collect();
    assert_eq!(found, [("fixture-parts".to_string(), "python"), ("web".to_string(), "typescript")]);
    let atlas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    assert_eq!(roots(&atlas, &Options::default()).len(), 1, "the workspace is one root; engine/tests holds data");
}

/// The parts side by side in one graph, every node in its part; checked part by part.
#[test]
fn the_parts_are_one_graph() {
    let g = shop();
    let parts: Vec<(&str, &str)> = g.parts.iter().map(|p| (p.id.as_str(), p.language.as_str())).collect();
    assert_eq!(parts, [("shop", "python"), ("web", "typescript")]);
    assert_eq!(g.language, "", "read in two languages");
    assert_eq!(g.node("shop.api.item").unwrap().part, "shop");
    assert_eq!(g.node("web.lib.client.item").unwrap().part, "web");
    let each: Vec<String> = analyze_parts(&[fixture()], &Options::default()).unwrap().into_iter().map(|g| g.language).collect();
    assert_eq!(each, ["python", "typescript"]);
}

/// A route is the path a client asks for: under the prefix its app is mounted at, and the
/// prefixes its router is declared and included with.
#[test]
fn routes_are_served_at_their_full_path() {
    let g = shop();
    assert_eq!(label(&g, "shop.api.item"), "GET /api/items/{item_id}", "`app.mount(\"/api\", items)`");
    assert_eq!(label(&g, "shop.serve.create_order"), "POST /v1/orders", "`include_router(router, prefix=\"/v1\")` and `APIRouter(prefix=\"/orders\")`");
    assert_eq!(label(&g, "shop.serve.health"), "GET /health");
    assert_eq!(g.node("web.routes.health").unwrap().entry.as_ref().unwrap().kind, EntryKind::Route);
}

/// A literal that is another part's route is a request to its handler; a part's own page of
/// the same name is a link within it.
#[test]
fn a_client_requests_what_a_server_serves() {
    let g = shop();
    let requests: Vec<(&str, &str, &str)> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Requests)
        .map(|e| (e.src.as_str(), e.dst.as_str(), e.via.as_deref().unwrap_or("")))
        .collect();
    assert!(requests.contains(&("web.lib.client.item", "shop.api.item", "/api/items/{item_id}")), "{requests:?}");
    assert!(requests.contains(&("web.lib.client.order", "shop.serve.create_order", "/v1/orders")), "{requests:?}");
    assert!(!requests.iter().any(|(_, dst, _)| *dst == "shop.serve.health"), "`/health` is web's own page: {requests:?}");
}
