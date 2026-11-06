//! The command's test: it keeps a mark that it ran.

#[test]
fn ran() {
    std::fs::write("out/ran.txt", "ok").unwrap();
}
