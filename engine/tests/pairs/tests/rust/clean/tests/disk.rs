//! The shelf's tests, beside the crate: a sample shelf on disk, loaded through the crate's door.

fn sample() -> String {
    String::from("fixtures/shelf.txt")
}

#[test]
fn test_loads() {
    assert!(!shelf::load(&sample()).is_empty());
}
