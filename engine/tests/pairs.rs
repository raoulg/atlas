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

/// Services that name their package after themselves (`orders`, `billing`) are parts by those
/// names, and every id is the code's own. Services that each call their package `app` are named by
/// what the code does not say, the directory each lies in (`orders/app`, `billing/app`), and both
/// stay in the picture: the shortcut is the clean program with a directory where the code gives no
/// name, its code, calls and places alike. So in Python, Rust and TypeScript, whose package is named
/// by its `package.json`, as a Python package by its manifest and a Rust crate by its `Cargo.toml`.
#[test]
fn two_parts_are_named_apart() {
    for language in ["python", "rust", "typescript"] {
        let (clean, shortcut) = (pair(&format!("parts/{language}/clean")), pair(&format!("parts/{language}/shortcut")));
        let parts = |g: &Graph| g.parts.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(parts(&clean), ["billing", "orders"], "{language}");
        assert_eq!(parts(&shortcut), ["billing/app", "orders/app"], "{language}");
        // the shortcut's ids with the directory as the name the code does not give
        let named = |g: &Graph, name: &dyn Fn(&str) -> String| {
            let mut out: Vec<String> = g.nodes.iter().filter(|n| !n.library).map(|n| format!("{} {:?}", name(&n.id), n.touches)).collect();
            out.extend(g.edges.iter().map(|e| format!("{} {:?} {}", name(&e.src), e.kind, name(&e.dst))));
            out.sort();
            out
        };
        assert_eq!(named(&shortcut, &|id| id.replacen("/app", "", 1)), named(&clean, &|id| id.to_string()), "{language}");
    }
}

/// A client that names the paths it asks for is joined to the handlers that serve them, by
/// method and path: its POST to `/api/orders` is not the GET of that path. One that looks its
/// paths and methods up in tables by a key, with an environment variable that may stand in for
/// each path, asks an address the code does not say, and meets at most the routes at the paths its
/// tables hold. Python reads a table the code writes, looked up by a key: `PATHS[key]`, `key` a
/// parameter, is one of the paths the table holds, the variable standing in being an input with that
/// default, and `item` meets the one route at its path; the method `METHODS["order"]` is handed to the request where the code does not say it
/// as a method, so `order` meets each route at `/api/orders`. TypeScript and Rust do not read such
/// a table yet (#236), and their shortcut meets no route. So in TypeScript (`clean`, `shortcut`),
/// Python (`pyclean`, `pyshortcut`) and Rust (`rsclean`, `rsshortcut`).
#[test]
fn a_request_meets_the_route_it_names() {
    let g = pair("requests");
    let requests = |side: &str| -> Vec<(String, String)> {
        g.edges.iter().filter(|e| e.kind == EdgeKind::Requests && e.src.starts_with(side)).map(|e| (e.src.clone(), e.dst.clone())).collect()
    };
    // the routes at the paths the shortcut's tables hold, `/api/items/{..}` and `/api/orders`
    let tabled = ["shop.api.item", "shop.api.order", "shop.api.orders"];
    for (clean, shortcut) in [("clean", "shortcut"), ("pyclean", "pyshortcut"), ("rsclean", "rsshortcut")] {
        // a Rust crate's functions are its root module's
        let at = |side: &str, f: &str| if side.starts_with("rs") { format!("{side}.{f}") } else { format!("{side}.client.{f}") };
        let want = [(at(clean, "item"), "shop.api.item".to_string()), (at(clean, "order"), "shop.api.order".to_string())];
        assert_eq!(requests(&format!("{clean}.")), want);
        let asked = requests(&format!("{shortcut}."));
        assert!(
            asked.iter().all(|(_, route)| tabled.contains(&route.as_str())),
            "{shortcut}: a request meets a route its tables hold: {asked:?}"
        );
        if shortcut == "pyshortcut" {
            let picked = [
                (at(shortcut, "item"), "shop.api.item".to_string()),
                (at(shortcut, "order"), "shop.api.order".to_string()),
                (at(shortcut, "order"), "shop.api.orders".to_string()),
            ];
            assert_eq!(asked, picked, "the key picks the path; a method the code does not say meets every route at it");
        }
        for f in ["item", "order"].map(|f| at(shortcut, f)) {
            let net = g.node(&f).unwrap().touches.iter().find(|t| t.kind == World::Net).cloned();
            assert!(
                net.as_ref().map_or(false, |t| t.artifact.is_empty() && !t.fixed),
                "{f} asks an address the code does not say: {net:?}"
            );
        }
    }
}

/// A request's method says which way it goes, as a statement's verb does: a GET takes what the
/// place holds, a POST leaves something there. The clean clients say the method of each request
/// (`requests.post`, `fetch(url, { method: "POST" })`, `client.post(..)`); the shortcut looks the
/// method of `order` up by a key at run time (`requests.request(METHODS["order"], ..)`,
/// `{ method: method("order") }`, `client.request(method("order"), ..)`), so the code does not say
/// which way it goes and the place is in neither domain nor codomain. Its `item` still says GET.
#[test]
fn a_request_goes_the_way_its_method_says() {
    use atlas_engine::flow::Side;
    let g = pair("requests");
    let side =
        |f: &str| -> Vec<Side> { g.node(f).unwrap().touches.iter().filter(|t| t.kind == World::Net).map(|t| t.side()).collect::<Vec<_>>() };
    for (clean, shortcut) in [("clean", "shortcut"), ("pyclean", "pyshortcut"), ("rsclean", "rsshortcut")] {
        let at = |side: &str, f: &str| if side.starts_with("rs") { format!("{side}.{f}") } else { format!("{side}.client.{f}") };
        assert_eq!(side(&at(clean, "item")), [Side::In], "{clean}: a GET reads");
        assert_eq!(side(&at(clean, "order")), [Side::Out], "{clean}: a POST writes");
        assert_eq!(side(&at(shortcut, "item")), [Side::In], "{shortcut}: a GET reads");
        assert_eq!(side(&at(shortcut, "order")), [Side::Open], "{shortcut}: a method looked up at run time says no way");
    }
}
