use std::fs;

fn bill(text: &str) -> String {
    text.to_uppercase()
}

fn main() {
    fs::write("out/invoice.txt", bill(&fs::read_to_string("data/ledger.csv").unwrap())).unwrap();
}
