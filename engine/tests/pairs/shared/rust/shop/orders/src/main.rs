use std::fs;

fn main() {
    let orders = fs::read_to_string("data/orders.csv").unwrap();
    notekit::save("out/orders.txt", &orders);
}
