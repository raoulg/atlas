use std::fs;

fn take_orders() -> String {
    fs::read_to_string("data/orders.csv").unwrap()
}

fn main() {
    fs::write("out/orders.txt", take_orders()).unwrap();
}
