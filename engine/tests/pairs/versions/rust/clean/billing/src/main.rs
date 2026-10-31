use std::fs;

fn main() {
    let bills = fs::read_to_string("data/bills.csv").unwrap();
    notekit::save("out/bills.txt", &bills);
}
