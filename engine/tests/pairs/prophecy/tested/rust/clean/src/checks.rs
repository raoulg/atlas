use crate::seal;

pub fn sealed(r: &str) -> String {
    seal(r)
}

#[test]
fn a_sealed_report_keeps_its_text() {
    assert_eq!(sealed(" r "), "r");
}
