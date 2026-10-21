fn sealed(r: &str) -> String {
    archive::seal(r)
}

#[test]
fn a_report_is_sealed_from_outside() {
    assert_eq!(sealed("r"), "r");
}
